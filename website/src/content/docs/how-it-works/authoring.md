---
title: Authoring and repair
description: How Muse Spark drafts a block graph and repairs it against engine diagnostics and failing test scenarios.
---

Every model call the application makes goes to Muse Spark through the Meta Model API:
drafting, repair, reading a sequence, proposing a point binding, explanations and the benchmark.

## The loop

1. **Task.** Spark gets a spec's task text: its typed inputs and outputs, parameters and
   behavior, plus the authoring rules and the palette of block classes from the pinned engine's
   catalog.
2. **Draft.** Spark answers with a compact block-graph JSON, constrained by a strict
   structured-output schema (`POST /v1/responses`, model `muse-spark-1.3`).
3. **Check.** The draft goes through the [verification receipt](/mechanical-muse/how-it-works/verification/):
   interface, validator, emitter, engine load, then every scenario replayed.
4. **Repair.** A failing draft goes back with a specific message, and Spark tries again. The
   budget is fixed (up to 5 attempts in product sessions).

## What the repair message says

- **An engine refusal** carries the engine's typed diagnostic `code()`, such as `algebraic-loop`,
  `class-not-found` or `unresolved-reference`. An algebraic loop is named as a chain of block
  ids, with the fix: break the cycle with `Discrete.UnitDelay` (Real) or `Logical.Pre`
  (Boolean). On the pinned engine those are the only authorable classes that break a loop.
- **A replay failure** names the scenario, the output, the instant, the expected and actual
  values, the check window, and each input's value at that instant and when it last changed.

A loop rule in the system prompt names the two loop-breaking classes up front. Before it, every
first draft of the plant reset looped; after it, none of 9 drafts did (three sessions; see
[Benchmark results](/mechanical-muse/results/benchmark/)).

## Recorded sessions

Live sessions can be recorded (model replies only, no credentials) and played back without a
key. The checks still run live on playback, so a recording replays the real verdicts. The web
app labels every recording "recorded".

## In the web app

The authoring panel follows a session attempt by attempt over server-sent events: engine
refusals, failing scenarios with their instants, then the green receipt and content id. A
verified draft is stored with its receipt and can be run on BOPTEST or downloaded as CXF.

Muse Spark can also explain a graph or a run's events in plain words. Explanations are
model-written, labelled as such, and never used for control.

## Learn more

- [docs/model-api.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/model-api.md):
  the Meta Model API as we use it (structured output, tools, limits, retries).
- [docs/block-graph-schema.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/block-graph-schema.md):
  the block-graph format Spark writes.
- [Spark tools](/mechanical-muse/how-it-works/spark-tools/): letting Spark check its own drafts.
