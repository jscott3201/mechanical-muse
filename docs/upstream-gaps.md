# Upstream gaps

Things the pinned dependencies (open-control-engine `4063202`, rusty-bacnet `1caf4db`,
open-control-library `124427b`) do not do, or do differently from what we need. We never patch
them (AGENTS.md hard rule 2); each entry says how we work around it in an `mm-*` crate.

| Gap | Where | Workaround | Evidence |
| --- | --- | --- | --- |
| The export content id depends on the authored `@graph` node order, so two documents with the same nodes in a different order get different ids ([engine #338](https://github.com/jscott3201/open-control-engine/issues/338)). | open-control-engine `export_cxf().content_id_complete()` | Our emitter writes one fixed order, so our ids are stable. Never compare our ids with a library file's `verified.content_id` for rules whose node order differs from the emitter's (AHU-0023, CHW-0002, HW-0006, HW-0008 at `124427b`). | `crates/mm-bridge/tests/emitted_cxf_loads.rs::content_id_depends_on_node_order` |
| The library's verifier (`tools/verify`) targets the engine's removed tick API and pins engine `41e997f` ([library #103](https://github.com/jscott3201/open-control-library/issues/103)). | open-control-library | Vector replay is our own code in `mm-bridge`. On engine `4063202` it passes all 1,760 scenarios of the 137 library rules, so the library's vectors are valid ground truth for our rev. | `crates/mm-bridge/tests/replay.rs::whole_library_replays_on_the_pinned_engine` (run with `MM_LIBRARY_DIR`) |
| The catalog lists ports without names for the 25 width-driven classes and the 3 `TimeTable` sources. | open-control-engine catalog | `mm-graph` marks those classes not authorable until port synthesis exists. | `crates/mm-graph/src/palette.rs` tests |
| The `TrimAndRespond` oracle covers only `have_hol = false`; PlantReset needs `true`. | open-control-engine reference catalog | Compose from elementary blocks; our vectors are the oracle. | `docs/plant-reset.md` section 1.6 |
| A boundary input that no connection reads is dropped from `input_definitions()`, so vectors that set it fail with an unknown name ([engine #339](https://github.com/jscott3201/open-control-engine/issues/339)). | open-control-engine load | `mm-graph` flags it as `unused-input` before the engine sees the draft. | `crates/mm-bridge/tests/replay.rs::a_constant_false_graph_fails` (comment) |
| An algebraic loop fails at stage `schedule` with an empty diagnostic receipt, so there is no code to branch on ([engine #340](https://github.com/jscott3201/open-control-engine/issues/340)). | open-control-engine load | `mm-bridge` gives it our own code, `load-failed-schedule`. | `crates/mm-bridge/tests/replay.rs::an_algebraic_loop_gets_a_code_even_without_diagnostics` |
