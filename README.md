# ps12image_rust_operator

Pure-Rust port of [`pamgene/ps12image_operator`](https://github.com/pamgene/ps12image_operator).

Extracts PamStation-12 image **metadata** into a Tercen table: given a
`documentId` column factor referencing an image ZIP, it downloads the
archive, finds the `ImageResults/*.tif` images, reads each image's PS12
TIFF tags (barcode, row/col, cycle, exposure time, filter, temperature,
timestamps, instrument unit, run id) and saves one row per image. No pixel
data is decoded.

## Input

- A crosstab with a **documentId** column factor referencing the image ZIP
  (a PamStation export). One archive is processed — the first documentId —
  matching the R original.
- Accepted archive layouts:
  1. PamStation export: `<barcode>/ImageResults/*.tif` (preferred).
  2. Flat: `*.tif` at the archive root (fallback, with a warning — the R
     original crashed on these).

Test data: [`tercen/pamchip_grid_dataset`](https://github.com/tercen/pamchip_grid_dataset)
— `641129101/641129101_ImageResults.zip`.

## Output

One row per image, `.ci = 0` (matching the R original):
`documentId, path, Image, DateTime, Barcode, Col, Cycle, Exposure Time,
Filter, PS12, Row, Temperature, Timestamp, Instrument Unit, RunId` —
`Col/Cycle/Exposure Time/Row/Temperature` numeric, the rest strings,
all namespace-prefixed.

## Why the port

- **Clear errors**: the R original died with
  `no applicable method for 'mutate' applied to an object of class "NULL"`
  when an archive had no `ImageResults/` folder. This port names the
  problem and accepts flat archives.
- **Native-execution eligible** (Rust binary, trusted repo) — no container
  start-up overhead.
- No renv / R-runtime fragility.

## Development

```bash
# Local run against a workflow step (no task needed):
export TERCEN_URI=... TERCEN_TOKEN=... WORKFLOW_ID=... STEP_ID=...
OUTPUT_CSV=/tmp/out.csv cargo run --bin dev
```

`operator.json`'s `container` must pin the release tag before tagging
(see CLAUDE.md release rules).
