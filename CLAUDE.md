# ps12image_rust_operator — maintenance guide

Pure-Rust port of `pamgene/ps12image_operator` (R, 1.2.2): extracts PS12
TIFF metadata from a PamStation image ZIP into one table row per image.
No pixel processing.

## Architecture (mirrors tercen/pamsoft_grid_rust_operator)

- `src/input.rs` — resolve the documentId from the column-facet table
  (first row, matching R's `df$documentId[1]`).
- `src/download.rs` — FileService download, ZIP extract, TIFF discovery:
  `ImageResults/*.tif` preferred, root-level fallback (improvement over R,
  which crashed on flat archives), loud error when zero TIFFs.
- `src/tags.rs` — the domain knowledge: PS12 private TIFF tags
  65050..65062 → named fields (table in the module docs). Missing tag → "".
- `src/output.rs` — result DataFrame: `.ci=0`, explicit `.ri`, namespaced
  columns; `Col/Cycle/Exposure Time/Row/Temperature` as f64 (unparseable →
  null, matching R's `as.numeric` → NA).
- `src/upload.rs` — `save_table` via the task (copied verbatim from
  pamsoft_grid_rust_operator).

## Parity with the R original

The parity target is the R operator's actual output — generate it by
running `pamgene/ps12image_operator@1.2.2` in Tercen Studio on
`tercen/pamchip_grid_dataset` → `641129101/641129101_ImageResults.zip`,
export the output CSV, and diff. If the R output's tag columns are empty
strings (ijtiff may not surface the PS12 private tags), this port is a
superset: same columns, real values. Adjust `src/tags.rs` mappings only
against that evidence.

## The Tercen unit test (`tests/test.json`) — TODO before wide release

Golden files must come from a real run (deploy-tercen-library skill §5):
run this operator in Studio on the test archive, export output +
`.schema` sidecars, pin everything in `tests/test.json` (`equalityMethod
R2` for the numeric columns). The operator is deterministic (no RNG) —
no Seed setting needed.

## Release rules

1. Never point `operator.json`'s `container` at `:main`/`:latest`.
2. Before tagging `X.Y.Z`: set `"container":
   "ghcr.io/tercen/ps12image_rust_operator:X.Y.Z"`, commit, then tag the
   same `X.Y.Z`. The release workflow pushes the image before its
   install-check runs.
3. The install-check is fatal by design — a red release burns a patch
   number; fix and tag the next.
4. Public repo: the default `GITHUB_TOKEN` suffices everywhere (ghcr push
   + zipball install).

## Memory model

`memory_model.json`: 600 MB floor + 0.02 MB/cell. The dominant cost is
the in-memory ZIP (~60-120 MB) + extraction; metadata rows are trivial.
Refit against `stats_d_actual_ram_peak` task metas if exit-137 appears.
