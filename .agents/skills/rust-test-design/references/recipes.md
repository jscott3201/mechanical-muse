# Project test recipes

| Risk | Recipe |
|---|---|
| Emitter drift | One golden per sequence under `crates/mm-graph/tests/golden/`, compared as bytes. `UPDATE_GOLDEN=1` rewrites; review the diff. Also load the emitted bytes with a receipt and assert no records and no warnings. |
| Nondeterminism | Emit twice and compare bytes. Load twice and compare the export `content_id`. Replay the vectors twice and compare every output with `bit_eq`. |
| Wrong diagnostic | One test per known failure. Unknown class gives stage `Import` and `class-not-found`; a dangling connection gives `unresolved-reference`; a missing input column gives `FrameMissingInput`; time 60 then 0 gives `TimeRegression`. Keep a wildcard arm for `#[non_exhaustive]` errors. |
| Vector replay | Table of scenarios. Fresh engine per scenario. Hold-last lives in the harness: keep a current-value table and submit all of it each frame. Inclusive `from_s..to_s` windows; reals within tolerance (1e-9 default), booleans and integers exact. Unresolvable or ambiguous names are errors. |
| Weak oracle | Run a constant-false and a constant-true graph through a rule's vectors; each must fail at least one scenario. Mutate one threshold, one connection and one delay; each mutation must turn some scenario red. |
| Model output | A corpus file of bad drafts (truncated JSON, wrong enum, unknown class, duplicate ids, dangling connection, NaN literal, oversized string). Each yields a typed error and a repair message. |
| Fake plant | A struct implementing the plant trait with scripted measurements. Cases: refused frame, stale sensor, NaN, error from `/advance`. Assert no new setpoint writes, hold or revert behavior, and the degraded flag. |
| Quantization | f32 quantization happens once, in `mm-bacnet`. Assert the quantized value at that point; everywhere else values stay `f64`. |
| Live tests | `#[ignore]` plus an env check that skips with a clear message. They never run in the default suite and never print keys. |

<!-- Adapted from yet-more-skills/rust-test-design (MIT OR Apache-2.0) -->
