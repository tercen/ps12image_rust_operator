//! Local memory bench: runs the operator's disk-side pipeline (zip extract →
//! TIFF tag read → result DataFrame) over a given archive, skipping only the
//! tercen download/upload legs (both are streaming/tiny). Measure with
//! `/usr/bin/time -v` for max RSS.
use std::io::Write;
use std::path::{Path, PathBuf};

use ps12image_operator::{output, tags};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let archive = PathBuf::from(&args[1]);
    let work = std::env::temp_dir().join("ps12_membench");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    // extract entry-by-entry, same as download::extract_zip
    let reader = std::fs::File::open(&archive)?;
    let mut za = zip::ZipArchive::new(reader)?;
    for i in 0..za.len() {
        let mut entry = za.by_index(i)?;
        let name = entry.enclosed_name().unwrap().to_path_buf();
        let outpath = work.join(&name);
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut out = std::fs::File::create(&outpath)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    drop(za);

    let mut images: Vec<PathBuf> = Vec::new();
    walk(&work, &mut |p| {
        if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("tif")).unwrap_or(false) {
            images.push(p.to_path_buf());
        }
    })?;
    images.sort();

    let mut records = Vec::with_capacity(images.len());
    for p in &images {
        records.push(tags::read_image_record(p)?);
    }
    let df = output::build_result_df(&records, "bench-doc", "ds0")?;
    writeln!(std::io::stdout(), "images={} rows={} cols={}", images.len(), df.height(), df.width())?;
    let _ = std::fs::remove_dir_all(&work);
    Ok(())
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path)) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() { walk(&p, f)?; } else { f(&p); }
    }
    Ok(())
}
