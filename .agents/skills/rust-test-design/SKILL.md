---
name: rust-test-design
description: "Guides writing Rust regression, golden, determinism and fault-injection tests that can actually fail. Use when adding or strengthening tests in any mm-* crate."
license: MIT OR Apache-2.0
metadata:
  short-description: "Rust tests that can actually fail"
---
# Rust Test Design

Name the behavior that could be wrong and the observable result that separates it from correct behavior, then write the smallest test for it. A bug fix lands with the test that would have caught it; confirm it fails first when practical.

## Rules for this workspace
- Offline by default: tests never call the Meta API or BOPTEST. Use scrubbed recorded fixtures behind `ModelClient`; live tests are `#[ignore]`, env-gated, and run with `cargo test --workspace -- --ignored`.
- Emitter goldens: graph JSON to CXF is byte-identical across runs. Regenerate only with `UPDATE_GOLDEN=1`, review the diff, and never regenerate to turn a red test green.
- Determinism: load the same CXF twice, run the same vectors, and assert identical outputs with `Value::bit_eq` (`Value` has no `PartialEq`) and an identical export `content_id`.
- Diagnostics: assert `stage()` and `code()` (`class-not-found`, `unresolved-reference`) or the exact error variant (`FrameMissingInput`), never message text; check side effects, not just `is_err()`.
- Vector replay: a fresh engine per scenario, every input every frame, windowed expectations, tolerance for reals. A deliberately broken graph must fail.
- Runtime: drive fail-safe paths (refused frame, stale data, NaN sensor) with a fake plant behind the same trait; assert no new setpoint is written and the state is degraded.
- Model output: keep a malformed corpus (truncated JSON, wrong enum, duplicate ids, dangling connection, NaN literal) and assert a typed rejection, never a panic.

## Make the test able to fail
- Cover boundaries, not many arbitrary cases: empty, one, max, over-limit (8 MiB, 64 nesting levels), time going backwards, exact deadline edges, cancellation mid-await, cleanup after failure.
- A round trip or a stored snapshot of our own output proves determinism, not correctness. Take expected values from the spec, a hand table or an independent model (`building-fdd-validation`).
- Imagine one plausible wrong implementation (swapped comparison, ignored id, dropped connection, missing `delayOnInit`) and check the test catches it. A constant-false graph passes 40.9% of benchmark scenarios, so a test that cannot fail is a defect. Restore any experimental mutation.
- Tests run in parallel: unique temp dirs and ports, no ordering, no shared `Engine`. Use no wall clock, sort explicitly, and never iterate a `HashMap` into expected output.
- Run focused tests first and name any check not run.

Recipes: [project recipes](references/recipes.md)

<!-- Adapted from yet-more-skills/rust-test-design (MIT OR Apache-2.0) -->
