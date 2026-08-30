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

## Memory

The operator is constant-RAM by construction: the archive is streamed to
disk (one gRPC chunk in RAM), the ZIP is extracted entry-by-entry with a
small fixed buffer, TIFF **tags** are read via the IFD without decoding any
pixel data, and the result table is ~200 bytes per image.

Measured with `cargo run --release --bin membench <archive.zip>` under
`/usr/bin/time -v` (extract → tag read → DataFrame, the disk-side pipeline):

| archive                          | images | max RSS  |
|----------------------------------|--------|----------|
| tests/ps12_test_images.zip (1.2M)|      4 | 6.1 MiB  |
| synthetic 311M zip / 2.7G on disk|   1000 | 6.25 MiB |

(`/usr/bin/time -v` reports max RSS in KiB, so these are MiB too.)

`memory_model.json` therefore declares a **constant** model. Note the
server's estimate formula is `intercept * PROD(coef * feature^exp) +
offset * 1.5`, and its result is in **MiB**, not MB — `task_service.dart`
returns `memoryEstimateMB * (1024 * 1024)`, so `intercept: 200` books
209.7 MB. Two traps live in that formula: an `offset` is multiplied by
1.5, and with any feature present whose value is 0 (e.g. `n_main` on a
documentId-only crosstab) the multiplicative part collapses to 0. A
featureless constant `intercept` avoids both. 200 MiB = measured core +
headroom for the tokio/tonic runtime and cgroup writeback pressure;
archive size lands on disk + reclaimable page cache, never anonymous RAM.

Also note: on tercen < 1.0.22 the model only applies when the parent cube
query is already cached at submit (fresh runs booked via the legacy
`base_memory + uncompressed_size * ratio`, ~1.7 GB for a typical export);
from 1.0.22 the model applies on every run.
