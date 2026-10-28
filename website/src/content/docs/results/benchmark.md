---
title: Benchmark results
description: How well Muse Spark rebuilds open fault-detection rules and our own control sequences, scored by replaying test vectors through the pinned engine.
---

:::caution[Keep these beside the numbers]
- **Conformance, not equivalence.** Passing a rule's vectors means the graph behaves as those
  author-written scenarios expect, not that it equals the reference everywhere.
- **The library is public**, so its rules may be in pretraining data. Weak evidence against
  verbatim recall: none of the 396 verified graphs in the whole-library run reuses all of the
  reference's block ids, and 29 % use a different number of blocks.
- **Small samples on our own specs:** three runs or sessions each. We report counts, not rates.
:::

## Library fault rules

Muse Spark rebuilds open-control-library's 137 fault-detection rules from scrubbed prose tasks
(no block class names, no block ids, no other rule ids), and each draft is scored by replaying
the library's 1,760 test vectors through the pinned engine. Before scoring, all 1,760 scenarios
were replayed with the reference rules and all pass.

**Run of 2026-10-27:** `muse-spark-1.3-contributor` (the library is public), medium effort, up to
3 attempts, validator-only repair feedback (no vector results reach the model), zero-shot, tier
A (prose only), 3 runs, engine `fa5cb19`.

| Run | First try (of 137) | After repair (of 137) | Scenarios passed (of 1,760) |
| --- | --- | --- | --- |
| 2026-10-26, engine `4063202`, before the loop rule | 125, 121, 124 | 131, 132, 133 | 1,750, 1,744, 1,749 |
| 2026-10-27, engine `fa5cb19`, with the loop rule | 122, 125, 124 | 132, 130, 131 | 1,747, 1,737, 1,743 |

**Baselines:** a constant-false graph passes 0 rules and 720 of 1,760 scenarios; constant-true
passes 0 rules and 30 scenarios. The run-to-run spread (3 to 4 rules) is as large as the
difference between the two batches.

Where it fails (2026-10-27 run, rule-runs):

| Group | Rule-runs | First try | After repair |
| --- | --- | --- | --- |
| Statistical rules (CUSUM, baselines, rolling statistics) | 69 | 71% | 87% |
| Plain threshold/timing rules | 339 | 94% | 97% |
| 17+ blocks | 24 | 75% | 75% |
| 10-16 blocks | 90 | 74% | 91% |
| 5-9 blocks | 222 | 95% | 98% |
| 4 or fewer blocks | 75 | 100% | 100% |
| Rules using `Discrete.Sampler` | 21 | 48% | 71% |

Typical misses are statistical baselines, evaluation gating and sampled signals.

### Held-out test set

Runs of 2026-10-26 on engine `4063202`; re-checked on `fa5cb19` with every verdict unchanged.

| Subset | Tier | Rules | First try | After repair | Scenarios (best attempt) | Constant-false |
| --- | --- | --- | --- | --- | --- | --- |
| test_clean | A | 37 | 33, 32, 32 | 35, 35, 36 | 475, 475, 477 of 478 | 0 rules; 204 of 478 |
| test_clean | B | 29 | 29, 29, 29 | 29, 29, 29 | 381 of 381 | 0 rules; 157 of 381 |

Tier B adds the card's pseudo-code, which is close to a direct specification, and skips 8 rules
whose pseudo-code names a block class or id; it is not comparable with tier A. The held-out set
is small (37 rules, only one over 16 blocks), which is why the whole-library run leads this page.

## Control sequences: our own specs

Unpublished specs, `muse-spark-1.3` (standard tier), repair feedback that names failing
scenarios. A pass after that kind of repair is "repaired against the scoring vectors", not a
blind score.

| Spec | Setup | First try | After repair | Constant baselines |
| --- | --- | --- | --- | --- |
| Chilled-water plant reset, v2 task | high effort, up to 5 attempts, 3 runs (2026-10-27) | 0 of 3 | 2 of 3 | 0 of 10 scenarios |
| Chilled-water plant reset, live sessions with the loop rule | high effort, up to 5 attempts, 3 sessions (2026-10-27) | | 3 of 3; 0 of 9 drafts looped | |
| Air handler chilled-water requests | medium effort, 3 runs (2026-10-26) | 3 of 3 | 3 of 3 | 1 of 15 scenarios |
| Zone static pressure requests, v2 | high effort, up to 5 attempts, 3 runs (2026-10-27) | 0 of 3 | 3 of 3 | 1 of 9 scenarios |
| Duct static pressure reset | high effort, up to 5 attempts, 3 runs (2026-10-27) | 3 of 3 | 3 of 3 | 0 of 6 scenarios |
| Home price response | high effort, up to 5 attempts, 3 runs (2026-10-28) | 3 of 3 | 3 of 3 | 0 of 17 scenarios |

- The plant reset is the hard one. Most first drafts had an algebraic loop until the repair
  message named the loop as a block chain and the prompt named the two classes that break one.
  With validator-only feedback (no vector results), only 1 of 12 runs produced a passing graph.
- The air handler requests and home price response are easy specs for the model: their tasks
  state every edge or formula.

## Learn more

- [docs/benchmark-results.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/benchmark-results.md):
  every run, rule lists, tokens and cost.
- [docs/benchmark-split.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/benchmark-split.md):
  the split and scrubbing rules.
- [docs/results-log.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/results-log.md):
  every result, including the ones that went badly.
