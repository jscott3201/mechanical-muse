---
name: diagnosis-loop
description: "Guides diagnosing a failing test, flaky tick, schema rejection or bad model output through a minimal reproduction and falsifiable hypotheses. Use when the cause of a failure is unknown."
license: MIT OR Apache-2.0
metadata:
  short-description: "Reproduce, hypothesize and fix failures with evidence"
---
# Diagnosis Loop

Explain the cause with evidence before changing code. A diagnosis-only request covers inspection and safe checks, not tracked edits; a request to fix includes the fix.

## Build a feedback loop
- State the expected behavior, the actual symptom and the observable failure signal. Inspect the source and callers first.
- Use the narrowest existing signal: one `cargo test -p <crate> <name>`, a recorded fixture replayed through `ModelClient`, one vector scenario, one CXF load receipt, or a captured BOPTEST payload. Keep the first failure output.
- Reproduce offline. Do not call the Meta API or BOPTEST for what a fixture shows; live runs need `-- --ignored` plus env keys, and keys never go into logs or fixtures.
- Minimize the graph, vector or frame sequence while the symptom persists. If it cannot be reproduced, reason statically and label the missing runtime evidence.

## Test competing explanations
- Keep only as many hypotheses as the uncertainty needs, each with an observation that would falsify it. Change one variable at a time, cheapest probe first, and retire what the evidence contradicts.
- Engine and loader failures: read `stage()` and `code()` from the receipt, never message text. The engine reports only the first frame refusal, so fix that one cause and re-prepare.
- Flaky results: suspect `HashMap` order, wall-clock time, unseeded randomness, a shared `Engine` or a shared BOPTEST container before blaming the environment. Retries do not fix flakiness.
- BOPTEST `/advance` mutates the plant and is not idempotent; never replay a tick to look again.
- Prefer existing `tracing` output and focused asserts over broad logging, and remove temporary instrumentation before finishing.

## Stop at the right boundary
- A diagnosis ends with the cause (or bounded uncertainty), the reproduction, the evidence and the regression test that would have caught it.
- A fix corrects the root cause, lands that regression test and re-runs the failing path. Do not promote an untested theory to fact or widen a local defect into a refactor.

<!-- Adapted from yet-more-skills/diagnosis-loop (MIT OR Apache-2.0) -->
