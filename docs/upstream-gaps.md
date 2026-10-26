# Upstream gaps

Things the pinned dependencies (open-control-engine `4063202`, rusty-bacnet `1caf4db`,
open-control-library `124427b`) do not do, or do differently from what we need. We never patch
them (AGENTS.md hard rule 2); each entry says how we work around it in an `mm-*` crate.

| Gap | Where | Workaround | Evidence |
| --- | --- | --- | --- |
| The export content id depends on the authored `@graph` node order, so two documents with the same nodes in a different order get different ids. | open-control-engine `export_cxf().content_id_complete()` | Our emitter writes one fixed order, so our ids are stable. Never compare our ids with a library file's `verified.content_id` for rules whose node order differs from the emitter's (AHU-0023, CHW-0002, HW-0006, HW-0008 at `124427b`). | `crates/mm-bridge/tests/emitted_cxf_loads.rs::content_id_depends_on_node_order` |
| The library's verifier (`tools/verify`) targets the engine's removed tick API and pins engine `41e997f`. | open-control-library | Vector replay is our own code in `mm-bridge`; re-run all 137 rules on our engine rev before scoring models. | `docs/engine-api.md` section 13 |
| The catalog lists ports without names for the 25 width-driven classes and the 3 `TimeTable` sources. | open-control-engine catalog | `mm-graph` marks those classes not authorable until port synthesis exists. | `crates/mm-graph/src/palette.rs` tests |
| The `TrimAndRespond` oracle covers only `have_hol = false`; PlantReset needs `true`. | open-control-engine reference catalog | Compose from elementary blocks; our vectors are the oracle. | `docs/plant-reset.md` section 1.6 |
