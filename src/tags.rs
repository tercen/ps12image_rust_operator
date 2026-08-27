//! PS12 TIFF metadata extraction.
//!
//! The PamStation-12 writes its acquisition metadata as PRIVATE TIFF tags
//! in the 65050..65062 range (verified against real instrument exports,
//! e.g. `tercen/pamchip_grid_dataset` barcode 641129101):
//!
//! | tag   | field            | example        |
//! |-------|------------------|----------------|
//! | 65050 | Barcode          | 641129101      |
//! | 65051 | Col              | 1              |
//! | 65052 | Cycle            | 32..94         |
//! | 65053 | Exposure Time    | 10/50/100/200  |
//! | 65054 | Filter           | 1              |
//! | 65055 | PS12             | "PS12"         |
//! | 65058 | Row              | 1              |
//! | 65059 | Temperature      | 29/30          |
//! | 65060 | Timestamp        |                |
//! | 65061 | Instrument Unit  | 07010030       |
//! | 65062 | RunId            | 4293E34236C5   |
//!
//! `DateTime` is the standard TIFF tag 306. Missing tags become empty
//! strings — same forgiving behaviour as the R original (`ifelse(is.null(x),
//! "", x)`), so archives from older instrument firmware still produce rows.
//!
//! The mapping is validated against the R operator's output (golden test);
//! adjust here if parity testing shows ijtiff surfaced different fields.

use anyhow::{Context, Result};
use std::path::Path;
use tiff::decoder::Decoder;
use tiff::tags::Tag;

/// One image's metadata record, field names matching the R operator's
/// output columns.
#[derive(Debug, Clone, Default)]
pub struct ImageRecord {
    /// Filename stem (no `.tif`), e.g. `641129101_W1_F1_T50_P32_I2_A29`.
    pub image: String,
    pub date_time: String,
    pub barcode: String,
    pub col: String,
    pub cycle: String,
    pub exposure_time: String,
    pub filter: String,
    pub ps12: String,
    pub row: String,
    pub temperature: String,
    pub timestamp: String,
    pub instrument_unit: String,
    pub run_id: String,
}

const TAG_BARCODE: u16 = 65050;
const TAG_COL: u16 = 65051;
const TAG_CYCLE: u16 = 65052;
const TAG_EXPOSURE_TIME: u16 = 65053;
const TAG_FILTER: u16 = 65054;
const TAG_PS12: u16 = 65055;
const TAG_ROW: u16 = 65058;
const TAG_TEMPERATURE: u16 = 65059;
const TAG_TIMESTAMP: u16 = 65060;
const TAG_INSTRUMENT_UNIT: u16 = 65061;
const TAG_RUN_ID: u16 = 65062;

/// Read the PS12 metadata record from one TIFF file. Never fails on
/// missing tags — only on an unreadable/corrupt TIFF container.
pub fn read_image_record(path: &Path) -> Result<ImageRecord> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("open {}", path.display()))?;
    let reader = std::io::BufReader::new(file);
    let mut dec = Decoder::new(reader)
        .with_context(|| format!("decode TIFF header of {}", path.display()))?;

    let image = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();

    Ok(ImageRecord {
        image,
        date_time: tag_string(&mut dec, Tag::DateTime),
        barcode: tag_string(&mut dec, Tag::Unknown(TAG_BARCODE)),
        col: tag_string(&mut dec, Tag::Unknown(TAG_COL)),
        cycle: tag_string(&mut dec, Tag::Unknown(TAG_CYCLE)),
        exposure_time: tag_string(&mut dec, Tag::Unknown(TAG_EXPOSURE_TIME)),
        filter: tag_string(&mut dec, Tag::Unknown(TAG_FILTER)),
        ps12: tag_string(&mut dec, Tag::Unknown(TAG_PS12)),
        row: tag_string(&mut dec, Tag::Unknown(TAG_ROW)),
        temperature: tag_string(&mut dec, Tag::Unknown(TAG_TEMPERATURE)),
        timestamp: tag_string(&mut dec, Tag::Unknown(TAG_TIMESTAMP)),
        instrument_unit: tag_string(&mut dec, Tag::Unknown(TAG_INSTRUMENT_UNIT)),
        run_id: tag_string(&mut dec, Tag::Unknown(TAG_RUN_ID)),
    })
}

/// Fetch a tag and render it as a display string; missing tag → "".
fn tag_string<R: std::io::Read + std::io::Seek>(
    dec: &mut Decoder<R>,
    tag: Tag,
) -> String {
    match dec.get_tag(tag) {
        Ok(value) => value_to_string(&value),
        Err(_) => String::new(),
    }
}

/// Render a TIFF tag value the way the instrument wrote it: ASCII values
/// verbatim (trailing NULs trimmed), numeric scalars as plain numbers,
/// numeric lists space-joined.
fn value_to_string(v: &tiff::decoder::ifd::Value) -> String {
    use tiff::decoder::ifd::Value::*;
    match v {
        Ascii(s) => s.trim_end_matches('\0').trim().to_string(),
        Byte(x) => x.to_string(),
        Short(x) => x.to_string(),
        Signed(x) => x.to_string(),
        SignedBig(x) => x.to_string(),
        Unsigned(x) => x.to_string(),
        UnsignedBig(x) => x.to_string(),
        Float(x) => x.to_string(),
        Double(x) => x.to_string(),
        Rational(n, d) => {
            if *d != 0 && n % d == 0 {
                (n / d).to_string()
            } else if *d != 0 {
                format!("{}", *n as f64 / *d as f64)
            } else {
                format!("{n}/{d}")
            }
        }
        SRational(n, d) => {
            if *d != 0 && n % d == 0 {
                (n / d).to_string()
            } else if *d != 0 {
                format!("{}", *n as f64 / *d as f64)
            } else {
                format!("{n}/{d}")
            }
        }
        List(items) => items
            .iter()
            .map(value_to_string)
            .collect::<Vec<_>>()
            .join(" "),
        other => format!("{other:?}"),
    }
}
