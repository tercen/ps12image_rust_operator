//! Result DataFrame construction.
//!
//! Schema mirrors the R operator's output (`main.R::doc_to_data` +
//! `mutate(.ci = 0)` + `ctx$addNamespace()`):
//!
//!   * `.ci` (int32) — 0 for every row (the R original hardcodes it).
//!   * `.ri` (int32) — 0..n-1. R relies on `ctx$save()` assigning row
//!     indices; tercen-rs' `save_table` is literal, so emit explicitly.
//!   * `{ns}.documentId`, `{ns}.path` ("ImageResults"), `{ns}.Image`
//!   * `{ns}.DateTime`, `{ns}.Barcode`, `{ns}.Filter`, `{ns}.PS12`,
//!     `{ns}.Timestamp`, `{ns}.Instrument Unit`, `{ns}.RunId` — strings
//!   * `{ns}.Col`, `{ns}.Cycle`, `{ns}.Exposure Time`, `{ns}.Row`,
//!     `{ns}.Temperature` — f64 (R: `mutate_at(..., as.numeric)`;
//!     unparseable/empty → null, matching R's NA)

use anyhow::Result;
use polars::prelude::*;

use crate::tags::ImageRecord;

pub fn build_result_df(
    records: &[ImageRecord],
    doc_id: &str,
    namespace: &str,
) -> Result<DataFrame> {
    let n = records.len();
    let ns = |name: &str| format!("{namespace}.{name}");

    let ci: Vec<i32> = vec![0; n];
    let ri: Vec<i32> = (0..n as i32).collect();
    let document_id: Vec<String> = vec![doc_id.to_string(); n];
    let path: Vec<String> = vec!["ImageResults".to_string(); n];

    let image: Vec<String> = records.iter().map(|r| r.image.clone()).collect();
    let date_time: Vec<String> = records.iter().map(|r| r.date_time.clone()).collect();
    let barcode: Vec<String> = records.iter().map(|r| r.barcode.clone()).collect();
    let filter: Vec<String> = records.iter().map(|r| r.filter.clone()).collect();
    let ps12: Vec<String> = records.iter().map(|r| r.ps12.clone()).collect();
    let timestamp: Vec<String> = records.iter().map(|r| r.timestamp.clone()).collect();
    let instrument_unit: Vec<String> =
        records.iter().map(|r| r.instrument_unit.clone()).collect();
    let run_id: Vec<String> = records.iter().map(|r| r.run_id.clone()).collect();

    let col: Vec<Option<f64>> = records.iter().map(|r| parse_num(&r.col)).collect();
    let cycle: Vec<Option<f64>> = records.iter().map(|r| parse_num(&r.cycle)).collect();
    let exposure: Vec<Option<f64>> =
        records.iter().map(|r| parse_num(&r.exposure_time)).collect();
    let row: Vec<Option<f64>> = records.iter().map(|r| parse_num(&r.row)).collect();
    let temperature: Vec<Option<f64>> =
        records.iter().map(|r| parse_num(&r.temperature)).collect();

    let df = DataFrame::new(vec![
        Column::new(".ci".into(), ci),
        Column::new(".ri".into(), ri),
        Column::new(ns("documentId").into(), document_id),
        Column::new(ns("path").into(), path),
        Column::new(ns("Image").into(), image),
        Column::new(ns("DateTime").into(), date_time),
        Column::new(ns("Barcode").into(), barcode),
        Column::new(ns("Col").into(), col),
        Column::new(ns("Cycle").into(), cycle),
        Column::new(ns("Exposure Time").into(), exposure),
        Column::new(ns("Filter").into(), filter),
        Column::new(ns("PS12").into(), ps12),
        Column::new(ns("Row").into(), row),
        Column::new(ns("Temperature").into(), temperature),
        Column::new(ns("Timestamp").into(), timestamp),
        Column::new(ns("Instrument Unit").into(), instrument_unit),
        Column::new(ns("RunId").into(), run_id),
    ])?;

    Ok(df)
}

/// R's `as.numeric(as.character(x))`: parse, or NA (null) when it can't.
fn parse_num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok()
}
