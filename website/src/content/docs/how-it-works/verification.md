---
title: Verification receipts
description: Why model output is untrusted, and what a green verification receipt checks before a draft can run.
---

Spark's drafts are data. Nothing a model writes reaches BOPTEST or the BACnet mirror unless it
has a recorded green verification receipt: a content id plus the vector results.

## What a receipt checks

The stages run in order; the first failure stops the check and names its stage and code.

| Stage | What it checks | Built by |
| --- | --- | --- |
| Interface | The graph's inputs and outputs match the spec's, by name and type | `mm-agent` |
| Validate | Every block class is in the engine's catalog, every port exists, every input is driven, types match | `mm-graph` |
| Emit | The graph becomes CXF bytes, deterministically: the same graph always gives the same bytes and content id | `mm-graph` |
| Load | open-control-engine loads the CXF and returns typed diagnostics; any warning fails the draft (the allowlist of tolerated warnings is empty) | `mm-bridge` (uses open-control-engine) |
| Replay | Every scenario of the spec's test vectors runs on a fresh engine, every input on every frame; outputs are checked in time windows, with a tolerance for reals | `mm-bridge` |

The result reads "passes X of Y scenarios". A failing scenario reports the first failing time,
the output, the expected and actual values and the window.

## Rules the gate keeps

- **Separate engines.** Authoring and verification use their own engine instances. The runtime
  engine loads only verified CXF.
- **Checked again before a run.** A drafted graph is re-checked (interface, validator, every
  vector replayed) before a run loads it, not trusted from an earlier session.
- **Deterministic.** No hash-map iteration order, wall-clock time or unseeded randomness in the
  emitter, validator or replay. Loading the same CXF twice gives identical outputs and content
  ids, and tests assert it.
- **Typed, not textual.** Code branches on diagnostic `code()` strings, never on message text.

## What a pass means, and what it does not

Passing every scenario means the graph behaves as those scenarios expect. It is conformance to
the vectors, not a proof that the sequence is correct or equivalent to a reference everywhere.
That is why each spec's vectors come from an independent reference model and are tested against
deliberately broken graphs (see [Sequences](/mechanical-muse/sequences/)).

When the repair message names a failing scenario's expected value, a later pass has been
"repaired against the scoring vectors" and is reported that way.

## Learn more

- [docs/engine-api.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/engine-api.md):
  the engine boundary, frames, diagnostics and vector replay.
- [docs/upstream-gaps.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/upstream-gaps.md):
  gaps in the pinned dependencies and how we work around them.
