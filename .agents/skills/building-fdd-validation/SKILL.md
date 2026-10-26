---
name: building-fdd-validation
description: "Guides test vectors and acceptance checks for fault-detection and control logic: preconditions, NO_EVAL, timing, secondary outputs. Use when writing or reviewing vectors, specs or benchmark rules."
license: MIT OR Apache-2.0
metadata:
  short-description: "Design vectors for fault-detection logic"
---
# Fault-Detection Vector Design

Start from the rule or sequence definition: our own `specs/<name>/spec.md`, or an open-control-library rule used as test data. Library rules are data only. Its own verifier targets an older engine API, so vector replay is our `mm-bridge` code (see `docs/engine-api.md`).

## When evaluation is meaningful
- Host gating belongs to `mm-runtime`, not the graph. An input that is missing, stale, invalid or unavailable means NO_EVAL, never "false" or "normal". Keep missing, invalid, stale and known-zero distinct, and test the precondition as well as the inequality.
- Do not invent thresholds, timers or hysteresis from similar equipment; list unresolved choices as open questions in the spec. The host sanitizes NaN and infinities before a frame is built.

## Vector checklist
- Per rule: steady normal state, each transition (rise, fall, recovery), values just below, at and above each threshold (mind `>` versus `>=`), timer expiry (false before the delay, true after), the first frame, a mode change, and re-arm after disable.
- Library convention: every `TrueDelay` sets `delayOnInit = true` (177 of 177), so vectors that start with an already-true input depend on it. State it in prompts or emitter defaults.
- Assert secondary outputs (evaluability and sub-condition flags); they carry about 40% of library assertions. Every input appears in every scenario because the engine needs every input every frame.
- Windows are inclusive and checked at every tick; leave a gap only where a timing edge is genuinely ambiguous. Reals use tolerance (1e-9 default); booleans and integers are exact.

## Independence and the benchmark
- Derive expected values from the spec, a hand table or an independent reference model, never from the graph or engine under test. A snapshot of our own output is regression evidence only.
- A constant-false graph passes 40.9% of library scenarios, so report that baseline beside every pass rate and give each rule at least one scenario asserting `true`.
- The held-out split in `docs/benchmark-split.md` is fixed; never re-roll it. Build prompts from scrubbed specs, draw exemplars from train only, and phrase results as "passes X of Y vectors".
- Findings are advisory. Synthetic vectors are regression evidence, not field validation or compliance, and demo numbers are never fabricated.
- Scenario example and benchmark hygiene: [vector notes](references/vectors.md)

<!-- Adapted from yet-more-skills/building-fdd-validation (MIT OR Apache-2.0) -->
