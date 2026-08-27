//! Input reading: resolve the image-archive `documentId` from the
//! column-facet table.
//!
//! Contract (matches the R operator, `main.R`):
//!
//!   * The crosstab MUST have a `documentId`-typed column factor
//!     (`if (!any(ctx$cnames == "documentId")) stop(...)`).
//!   * The operator processes ONE archive — the first row's documentId
//!     (`df$documentId[1]`). If several distinct documentIds are present
//!     we warn and use the first, exactly like R silently does.

use anyhow::{anyhow, bail, Context, Result};
use polars::prelude::*;
use tercen_rs::context::ContextBase;
use tercen_rs::tson_to_dataframe;

/// Resolve the single documentId this operator processes.
pub async fn load_document_id(ctx: &ContextBase) -> Result<String> {
    let col_table_id = ctx.cube_query().column_hash.clone();
    if col_table_id.is_empty() {
        bail!(
            "operator has no column-facet table (cube_query.column_hash is empty). \
             ps12image expects a documentId column factor referencing the image ZIP."
        );
    }

    let all_cnames = ctx
        .cnames()
        .await
        .map_err(|e| anyhow!("fetch column-facet schema: {e}"))?;
    let doc_col = all_cnames
        .iter()
        .find(|c| c.contains("documentId"))
        .cloned()
        .ok_or_else(|| {
            anyhow!(
                "Column factor documentId is required — add a documentId-typed \
                 factor referencing the image ZIP to the column projection. \
                 (columns present: {:?})",
                all_cnames
            )
        })?;

    let streamer = ctx.streamer();
    let col_tson = streamer
        .stream_tson(&col_table_id, Some(vec![doc_col.clone()]), 0, -1)
        .await
        .map_err(|e| anyhow!("stream column-facet table {col_table_id}: {e}"))?;
    let col_df = tson_to_dataframe(&col_tson).context("parse TSON column-facet payload")?;

    let doc_series = col_df
        .column(&doc_col)
        .map_err(|e| anyhow!("missing documentId column '{}': {}", doc_col, e))?
        .cast(&DataType::String)
        .context("cast documentId to string")?;
    let doc_chunked = doc_series.str().context("documentId is not a string column")?;

    let first = doc_chunked
        .get(0)
        .ok_or_else(|| anyhow!("column-facet table is empty — no documentId rows"))?
        .to_string();
    if first.is_empty() {
        bail!("first documentId value is empty");
    }

    // R silently ignores rows beyond the first; we at least say so.
    let distinct: std::collections::BTreeSet<&str> =
        doc_chunked.into_iter().flatten().collect();
    if distinct.len() > 1 {
        tracing::warn!(
            n_distinct = distinct.len(),
            used = %first,
            "multiple distinct documentIds in the input — processing only the \
             first one (same behaviour as the R operator)"
        );
    }

    Ok(first)
}
