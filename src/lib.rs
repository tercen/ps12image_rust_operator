//! ps12image_operator — pure-Rust port of `pamgene/ps12image_operator`.
//!
//! Extracts PamStation-12 image metadata into a Tercen table: given a
//! `documentId` column factor referencing an image ZIP, it downloads the
//! archive, finds the `ImageResults/*.tif` images (with a fallback to
//! root-level TIFFs for flat archives), reads each image's PS12 TIFF tags,
//! and saves one row per image. No pixel data is decoded.
//!
//! The crate exposes a single entry point, [`run`], shared between the
//! production binary (`src/main.rs`, invoked by Tercen with `--taskId` /
//! `--serviceUri` / `--token`) and the dev binary (`src/bin/dev.rs`, driven
//! by `TERCEN_*` environment variables).

pub mod download;
pub mod input;
pub mod output;
pub mod tags;
pub mod upload;

use std::sync::Arc;

use anyhow::{Context, Result};
use tercen_rs::context::ContextBase;
use tercen_rs::{DevContext, ProductionContext, TercenClient};

/// Production entry point: bootstraps a `ProductionContext` from a task ID.
pub async fn run(task_id: &str) -> Result<()> {
    tracing::info!("ps12image_operator starting (task_id={task_id})");
    let client = build_client().await?;
    let ctx = ProductionContext::from_task_id(client, task_id)
        .await
        .map_err(|e| anyhow::anyhow!("load task {task_id}: {e}"))?;
    execute(&ctx, Some(task_id)).await
}

/// Dev entry point: bootstraps a `DevContext` from a workflow/step pair.
pub async fn run_dev(workflow_id: &str, step_id: &str) -> Result<()> {
    tracing::info!(
        "ps12image_operator starting in dev mode \
         (workflow_id={workflow_id}, step_id={step_id})"
    );
    let client = build_client().await?;
    let ctx = DevContext::from_workflow_step(client, workflow_id, step_id)
        .await
        .map_err(|e| anyhow::anyhow!("load workflow {workflow_id} / step {step_id}: {e}"))?;
    execute(&ctx, None).await
}

async fn build_client() -> Result<Arc<TercenClient>> {
    let client = TercenClient::from_env()
        .await
        .map_err(|e| anyhow::anyhow!("connect to Tercen: {e}"))?;
    tracing::info!("connected to Tercen");
    Ok(Arc::new(client))
}

/// Pipeline implementation, generic over the context flavour.
async fn execute(ctx: &ContextBase, task_id: Option<&str>) -> Result<()> {
    tracing::info!(
        workflow = ctx.workflow_id(),
        step = ctx.step_id(),
        project = ctx.project_id(),
        namespace = ctx.namespace(),
        "context loaded"
    );

    // Stage 1: read the documentId from the column-facet table. The R
    // operator processes exactly one archive — the first documentId —
    // and we keep that contract (`main.R`: `df$documentId[1]`).
    let doc_id = input::load_document_id(ctx)
        .await
        .map_err(|e| anyhow::anyhow!("load input table: {e:#}"))?;
    tracing::info!(doc_id, "input documentId resolved");

    // Stage 2: download the archive and locate the image TIFFs.
    let work_root = std::env::temp_dir().join(format!(
        "ps12image_op_{}_{}",
        ctx.workflow_id(),
        ctx.step_id()
    ));
    let _drop_guard = TempDirGuard(work_root.clone());
    let images = download::fetch_images(ctx, &doc_id, &work_root)
        .await
        .map_err(|e| anyhow::anyhow!("file download: {e:#}"))?;
    tracing::info!(n_images = images.len(), "image TIFFs located");

    // Stage 3: read the PS12 metadata tags from every image.
    let mut records = Vec::with_capacity(images.len());
    for path in &images {
        let rec = tags::read_image_record(path)
            .with_context(|| format!("read TIFF tags from {}", path.display()))?;
        records.push(rec);
    }
    tracing::info!(n_records = records.len(), "TIFF metadata extracted");

    // Stage 4: build the result DataFrame (schema mirrors the R operator).
    let df = output::build_result_df(&records, &doc_id, ctx.namespace())
        .map_err(|e| anyhow::anyhow!("build result DataFrame: {e:#}"))?;
    tracing::info!(
        n_rows = df.height(),
        n_cols = df.width(),
        namespace = ctx.namespace(),
        "result DataFrame built"
    );

    // Stage 5: upload (production) or dump to CSV (dev, OUTPUT_CSV set).
    match task_id {
        Some(tid) => {
            upload::save_results(ctx, tid, &df)
                .await
                .map_err(|e| anyhow::anyhow!("upload result table: {e:#}"))?;
        }
        None => {
            if let Ok(path) = std::env::var("OUTPUT_CSV") {
                write_csv(&df, &path)
                    .map_err(|e| anyhow::anyhow!("dump result CSV to {path}: {e:#}"))?;
                tracing::info!(n_rows = df.height(), path, "dev mode: result written to CSV");
            } else {
                tracing::info!(
                    n_rows = df.height(),
                    "dev mode: skipping save_table. Set OUTPUT_CSV=<path> to dump \
                     the result DataFrame locally."
                );
            }
        }
    }

    Ok(())
}

/// Dump a DataFrame to CSV (dev mode only).
fn write_csv(df: &polars::frame::DataFrame, path: &str) -> Result<()> {
    use polars::prelude::*;
    let mut df = df.clone();
    let mut f =
        std::fs::File::create(path).with_context(|| format!("create CSV at {path}"))?;
    CsvWriter::new(&mut f)
        .include_header(true)
        .finish(&mut df)
        .map_err(|e| anyhow::anyhow!("CsvWriter: {e}"))?;
    Ok(())
}

/// Best-effort temp-dir cleanup on exit.
struct TempDirGuard(std::path::PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Initialise tracing once — called by both binaries.
pub fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();
}

/// Require an environment variable to be set; helpful error otherwise.
pub fn require_env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} environment variable not set"))
}
