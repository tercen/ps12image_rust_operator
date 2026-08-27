//! Archive download + image discovery.
//!
//! Downloads the documentId's bytes via `FileService.download`, extracts
//! the ZIP, and returns the image TIFF paths:
//!
//!   1. Preferred: every `*.tif` under an `ImageResults/` directory —
//!      the PamStation export layout the R operator requires.
//!   2. Fallback (improvement over R): if no `ImageResults/` entries
//!      exist, accept `*.tif` files anywhere in the archive, with a
//!      warning. Flat instrument zips then work instead of crashing.
//!
//! Zero TIFFs is a loud, actionable error either way — the R original
//! died here with `mutate applied to an object of class "NULL"`.

use anyhow::{anyhow, bail, Context, Result};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use tercen_rs::context::ContextBase;
use tonic::Request;

/// Download + unpack `doc_id`, return the sorted list of image TIFF paths.
pub async fn fetch_images(
    ctx: &ContextBase,
    doc_id: &str,
    work_root: &Path,
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(work_root)
        .with_context(|| format!("create work root {}", work_root.display()))?;

    tracing::info!(doc_id, "downloading file");
    let bytes = stream_file_bytes(ctx, doc_id).await?;
    tracing::info!(doc_id, bytes = bytes.len(), "download complete");

    let looks_like_zip =
        bytes.len() >= 4 && (&bytes[..4] == b"PK\x03\x04" || &bytes[..4] == b"PK\x05\x06");
    if !looks_like_zip {
        bail!(
            "documentId {} is not a ZIP archive ({} bytes, magic {:02x?}). \
             ps12image expects the PamStation image export ZIP.",
            doc_id,
            bytes.len(),
            &bytes[..bytes.len().min(4)],
        );
    }

    let extracted = work_root.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    extract_zip(&bytes, &extracted)
        .with_context(|| format!("extract zip for doc {doc_id}"))?;

    // Preferred layout: */ImageResults/*.tif (matches the R operator's
    // `grep('*/ImageResults/*', f.names)`).
    let mut images = collect_tiffs(&extracted, true)?;
    if images.is_empty() {
        // Fallback: any .tif in the archive (flat instrument exports).
        images = collect_tiffs(&extracted, false)?;
        if images.is_empty() {
            bail!(
                "no .tif images found in the archive (documentId {}). \
                 ps12image expects a PamStation export containing an \
                 'ImageResults' folder of .tif images (or, at minimum, \
                 .tif files at the archive root).",
                doc_id,
            );
        }
        tracing::warn!(
            n_images = images.len(),
            "archive has no ImageResults/ folder — falling back to \
             root-level .tif files (flat archive layout)"
        );
    }

    // Deterministic order: sort by full path.
    images.sort();
    Ok(images)
}

/// Collect `*.tif` paths under `root`. When `image_results_only` is set,
/// keep only paths with an `ImageResults` directory component.
fn collect_tiffs(root: &Path, image_results_only: bool) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(root, &mut |path| {
        let is_tif = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("tif") || e.eq_ignore_ascii_case("tiff"))
            .unwrap_or(false);
        if !is_tif {
            return;
        }
        if image_results_only {
            let in_image_results = path
                .components()
                .any(|c| c.as_os_str().to_string_lossy() == "ImageResults");
            if !in_image_results {
                return;
            }
        }
        out.push(path.to_path_buf());
    })?;
    Ok(out)
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path)) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, f)?;
        } else {
            f(&path);
        }
    }
    Ok(())
}

/// Stream `FileService::download(file_document_id)` to completion.
async fn stream_file_bytes(ctx: &ContextBase, doc_id: &str) -> Result<Vec<u8>> {
    use tercen_rs::client::proto::ReqDownload;

    let mut file_service = ctx
        .client()
        .file_service()
        .map_err(|e| anyhow!("acquire file service: {e}"))?;

    let req = Request::new(ReqDownload {
        file_document_id: doc_id.to_string(),
    });
    let mut stream = file_service
        .download(req)
        .await
        .map_err(|e| anyhow!("file_service.download({doc_id}) failed: {e}"))?
        .into_inner();

    let mut buf = Vec::new();
    while let Some(chunk) = stream
        .message()
        .await
        .map_err(|e| anyhow!("stream chunk for {doc_id}: {e}"))?
    {
        buf.extend_from_slice(&chunk.result);
    }
    if buf.is_empty() {
        bail!("documentId {} download returned 0 bytes", doc_id);
    }
    Ok(buf)
}

/// Extract a ZIP archive (in-memory) into `dest`.
fn extract_zip(bytes: &[u8], dest: &Path) -> Result<()> {
    let reader = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).context("open zip archive")?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .with_context(|| format!("zip entry {i}"))?;
        let name = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("zip entry {i} has invalid name"))?
            .to_path_buf();
        let outpath = dest.join(&name);
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&outpath)?;
            let mut data = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut data)?;
            out.write_all(&data)?;
        }
    }
    Ok(())
}
