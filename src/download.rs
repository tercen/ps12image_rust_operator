//! Archive download + image discovery.
//!
//! Streams the documentId's bytes via `FileService.download` to a temp
//! file on disk (constant RAM regardless of archive size), extracts
//! the ZIP entry-by-entry with a small fixed buffer, and returns the
//! image TIFF paths:
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
use std::io::{Read, Write};
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
    let archive_path = work_root.join("archive.zip");
    let archive_len = stream_file_to(ctx, doc_id, &archive_path).await?;
    tracing::info!(doc_id, bytes = archive_len, "download complete");

    let mut magic = [0u8; 4];
    let n_magic = {
        let mut f = std::fs::File::open(&archive_path)?;
        f.read(&mut magic)?
    };
    let looks_like_zip =
        n_magic >= 4 && (&magic == b"PK\x03\x04" || &magic == b"PK\x05\x06");
    if !looks_like_zip {
        bail!(
            "documentId {} is not a ZIP archive ({} bytes, magic {:02x?}). \
             ps12image expects the PamStation image export ZIP.",
            doc_id,
            archive_len,
            &magic[..n_magic.min(4)],
        );
    }

    let extracted = work_root.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    extract_zip(&archive_path, &extracted)
        .with_context(|| format!("extract zip for doc {doc_id}"))?;
    // The archive is extracted; free the disk copy before walking images.
    let _ = std::fs::remove_file(&archive_path);

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

/// Stream `FileService::download(file_document_id)` to `dest` on disk.
/// Returns the byte count. RAM use is one gRPC chunk at a time.
async fn stream_file_to(ctx: &ContextBase, doc_id: &str, dest: &Path) -> Result<u64> {
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

    let mut out = std::io::BufWriter::new(
        std::fs::File::create(dest)
            .with_context(|| format!("create {}", dest.display()))?,
    );
    let mut total: u64 = 0;
    while let Some(chunk) = stream
        .message()
        .await
        .map_err(|e| anyhow!("stream chunk for {doc_id}: {e}"))?
    {
        out.write_all(&chunk.result)?;
        total += chunk.result.len() as u64;
    }
    out.flush()?;
    if total == 0 {
        bail!("documentId {} download returned 0 bytes", doc_id);
    }
    Ok(total)
}

/// Extract a ZIP archive (on disk) into `dest`, streaming each entry.
fn extract_zip(archive_path: &Path, dest: &Path) -> Result<()> {
    let reader = std::fs::File::open(archive_path)
        .with_context(|| format!("open {}", archive_path.display()))?;
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
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(())
}
