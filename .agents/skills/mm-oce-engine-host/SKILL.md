---
name: mm-oce-engine-host
description: "Guides code that drives oce-api in mm-bridge: CXF load receipts, frame stepping, diagnostics, Value DTOs, vector replay. Use when touching the engine boundary, Engine ownership or the pinned rev."
license: MIT OR Apache-2.0
metadata:
  short-description: "oce-api host rules: frames, receipts, DTOs"
---
`mm-bridge` is the only crate that calls `oce-api` (v0.2.0, rev `fa5cb19`, read-only; gaps go in `docs/upstream-gaps.md`). Depth: `docs/engine-api.md`.

**Frames (HostTick)**
- Each tick submit the whole input table: every `input_definitions()` path once, exact `ValueType`, no String. `prepare_frame(t, &entries)` right before `execute_frame`; a prepared frame goes stale after reload or halt/set/resume (`StalePreparedFrame`): re-prepare.
- `t` is finite, nondecreasing f64 BOPTEST sim seconds, never wall clock. An `Err` leaves the engine unchanged, but an executed frame is a state transition: never resubmit one.
- Read step results from `CompletedFrame::outputs()`. `get_output`/`watch` return latest state (explainer internals only); `topology()` feeds the graph view.
- Unbounded Real inputs accept NaN and infinity: sanitize at the host. Enum ordinals are 1-based.

**Loading**
- `load_cxf_with_receipt`: `Err(OperationFailure)` has `stage()` and `diagnostics().records()` (empty for JSON errors: use `error()`); an `Ok` receipt can still hold warnings.
- Branch on `key().code()` and `stage()`, never `message()`. A warning fails the draft unless its code is allowlisted (list pending in `docs/DECISIONS.md`). `OcError` is `#[non_exhaustive]`: keep a wildcard arm.
- Same CXF loaded twice gives the same `export_cxf().content_id()`; a green receipt is that id plus vector results. Limits: 8 MiB, 64 levels.

**Engines**
- Fresh `Engine::in_memory()` per candidate and per vector scenario. The runtime engine is a separate instance that loads only CXF with a green receipt. One owner task per Engine; send DTOs over channels.
- `halt`/`set_param`/`resume` restarts the run and re-seeds stateful blocks: no live tuning; pass resets as boundary inputs.
- `Value` lacks `PartialEq` and `Serialize`: compare with `bit_eq`; serialize via `mm-bridge` DTOs (`WireValue`), non-finite reals as strings.

**Vector replay is our code**: the library's `cxf-verify` targets the removed tick API, so never build it or call ours "the library verifier". Semantics: `docs/engine-api.md` section 9.
- Fresh engine per scenario; the harness holds last values and submits every input every frame. Ticks `k*step_s`, `k = 0..=floor(horizon/step)`; windows inclusive; Real tolerance 1e-9; max 1,000,000 samples, 512 scenarios.
- Names: exact path, else a unique `.name`/`#name` suffix. Unset inputs error unless a scenario `defaults` map sets them. Result: pass, fail (first t, expected, actual) or error (code).
- Expected values never come from engine output; a constant-false graph and a broken graph must fail.
