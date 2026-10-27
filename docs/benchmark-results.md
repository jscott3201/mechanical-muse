# Benchmark results

How well Muse Spark rebuilds open-control-library fault rules from a scrubbed task, scored by
replaying each rule's library test vectors through the pinned engine. Method, split and hygiene
rules: `docs/benchmark-split.md` and `.agents/skills/mm-benchmark-hygiene/SKILL.md`. Harness:
`crates/mm-bench`. Before scoring, all 1,760 library scenarios were replayed on the judge engine
and all pass (`docs/upstream-gaps.md`), so every rule is in the denominator.

## Where it fails (run of 2026-10-26, all 137 rules, tier A, 3 runs)

With no examples in the prompt, the train/test split matters only for twins, so the whole library
is a fair stress test of shapes the small test set lacks (statistical rules, 10+ blocks).

| Group | Rule-runs | First try | After repair |
| --- | --- | --- | --- |
| Statistical rules (CUSUM, baselines, rolling statistics) | 69 | **67%** | 87% |
| Plain threshold/timing rules | 339 | 95% | 98% |
| 17+ blocks | 24 | **75%** | 83% |
| 10-16 blocks | 90 | **73%** | 92% |
| 5-9 blocks | 222 | 95% | 98% |
| 4 or fewer blocks | 75 | 100% | 100% |
| Rules using `Discrete.Sampler` | 21 | **33%** | |

- Rules that failed the first try in at least 2 of 3 runs: AHU-0023, AHU-0029, AHU-0037,
  CHW-0001, CHW-0002, CHW-0003, FCU-0001, FPB-0004, FPB-0006, HP-0006, HW-0006, HW-0008,
  VFD-0003.
- Rules still failing after repair in at least one run: AHU-0029, AHU-0037, CHW-0002, CHW-0003,
  FPB-0004, FPB-0006, HP-0006, HW-0006, HW-0008, VFD-0003.
- Every first-try failure but two was a behavior mismatch found by replay. The other two were
  validator findings. No draft failed the interface lock or engine load.
- Typical misses: secondary outputs that should stay false while a baseline is still being
  learned (FPB-0004), and evaluation gating such as "don't judge while the valve is at its
  control limit" (HP-0006).

Overall: first try 125, 121 and 124 of 137 rules; after repair 131, 132 and 133. Scenarios
passed by the best attempt: 1,750, 1,744 and 1,749 of 1,760. Constant-false passes 0 rules and
720 of 1,760 scenarios; constant-true passes 0 rules and 30 scenarios.

## Control sequence: chilled-water plant reset (run of 2026-10-26)

Our own spec (`specs/chw_plant_reset/`): trim and respond with a staging hold, driving the
chilled-water supply temperature and differential pressure setpoints.

**Validation before any model saw it:**
- A hand-built 33-block reference graph passes all 10 scenarios on the pinned engine.
- Twelve deliberate mistakes each fail at least one scenario (`crates/mm-bridge/tests/specs.rs`).
- The vectors come from an independent reference model, not from the engine.

**Run setup:** `muse-spark-1.3`, the standard tier (the spec is unpublished); up to 3 attempts;
validator-only repair feedback; 3 runs per effort.

| Task version | Effort | First try | After repair | Best scenarios per run |
| --- | --- | --- | --- | --- |
| v1 (prose) | medium | 0 of 3 | 0 of 3 | 8, 1, 8 of 10 |
| v1 (prose) | high | 0 of 3 | 1 of 3 | 1, 10, 8 of 10 |
| v2 (edge instants stated, plus a structural hint) | medium | 0 of 3 | 0 of 3 | 7, 8, 8 of 10 |
| v2 (edge instants stated, plus a structural hint) | high | 0 of 3 | 0 of 3 | 8, 8, 8 of 10 |

Constant baselines pass 0 of 10 scenarios.

- **1 of 12 runs produced a fully passing graph, and none passed on the first try.**
- **A second staging start is missed.** This is the most common replay failure. When staging
  starts again soon after the first hold ends, it should freeze updates again. The model's hold
  block keeps its default false-hold duration, which equals the true-hold duration, so it ignores
  a new start for 900 s after release. The task says each staging start freezes updates.
- **The feedback loop's store is the wrong block.** Most first drafts are refused by the engine
  as algebraic loops. In v2, every first draft in the earlier batch used `Discrete.TriggeredSampler`
  as the store; a triggered sampler passes its input through at the trigger instant, so it
  cannot break the loop. A unit delay is needed.
- **v1 misses edge instants.** The first change lands one update late, and a request that
  changes at an update instant is read one update late. v2 states these instants and adds one
  structural hint: the stored value may only change at updates.

**An earlier version of these results was wrong.** It used 8 scenarios whose input changes all
fell on the update grid and had a single staging pulse each, so a wrong false-hold duration
went undetected. On those weaker vectors it reported after-repair passes of 2 of 6 (v1) and 5 of
6 (v2). Review caught this; those numbers are withdrawn.

**Repair feedback bug, now fixed.** Engine refusals of algebraic loops carry no diagnostic records
(engine #340). The repair message dropped the engine's explanation in the earliest runs; it is
passed on in all runs reported here.

## Control sequence: air handler chilled-water requests (run of 2026-10-26)

Our own spec (`specs/ahu_chw_requests/`): one air handler's chilled-water reset requests (0 to
3) and chiller plant request, from the supply air temperature error and the cooling valve
position, with persistence timers, release bands and set/clear latches.

**Validation before any model saw it:** an 18-block reference graph passes all 15 scenarios on
the pinned engine; 22 deliberate mistakes each fail at least one scenario; the vectors come
from an independent reference model.

**Run setup:** as for the plant reset, medium effort, 3 runs.

| Task version | Effort | First try | After repair | Best scenarios per run |
| --- | --- | --- | --- | --- |
| v1 (edge instants stated) | medium | 3 of 3 | 3 of 3 | 15, 15, 15 of 15 |

Constant baselines pass 1 of 15 scenarios. (A first batch on an earlier 12-scenario version also
passed 3 of 3; review then found wrong graphs those 12 let through, and the vectors were
strengthened before this run.)

- **Every run passed on the first try.** The task states every edge instant (strict thresholds,
  a condition true at the first frame, a latch's set and clear in one frame), and the sequence
  maps onto elementary blocks one to one. It is an easy spec for the model; it exists to feed
  the plant reset in the live runtime, not to probe weaknesses.

## Held-out test set

| Subset | Tier | Rules | First try | After repair | Scenarios (best attempt) | Constant-false |
| --- | --- | --- | --- | --- | --- | --- |
| test_clean | A | 37 | 33, 32, 32 | 35, 35, 36 | 475, 475, 477 of 478 | 0 rules; 204 of 478 |
| test_strict | A | 24 | 20, 19, 20 | 22, 22, 23 | 314, 314, 316 of 317 | 0 rules; 126 of 317 |
| test_clean | B | 29 | 29, 29, 29 | 29, 29, 29 | 381 of 381 | 0 rules; 157 of 381 |
| test_strict | B | 20 | 20, 20, 20 | 20, 20, 20 | 263 of 263 | 0 rules; 104 of 263 |

- The test_strict rows are filters of the test_clean runs, not separate runs.
- Test rules that failed the first try at least once (tier A): AHU-0037, CHW-0002, FPB-0004,
  HP-0006, HW-0002, VFD-0003. FPB-0004 and HP-0006 also stayed unverified after repair in at
  least one run.
- Tier B is not comparable with tier A. It skips 8 test_clean rules whose pseudo-code names a
  block class or block id: AHU-0001, CHW-0009, ERV-0002, HW-0001, SYS-0010, TOWER-0003, VAV-0005,
  VFD-0003. Those include the counting, CUSUM and spike rules, which are among the hardest.

## Setup

| Setting | Value |
| --- | --- |
| Model | `muse-spark-1.3-contributor` (public library data only), effort `medium`, `max_output_tokens` 32,000 |
| Attempts | up to 3 per rule. Repair feedback is `validator-only`: validator and load problems in full, and a replay failure only as "behavior does not match", with no scenario names, counts or values. |
| Exemplars | none (zero-shot) |
| Runs | 3 independent runs per rule (no documented seed) |
| Judge | our replay in `mm-bridge` on open-control-engine `4063202b`; open-control-library `124427b` |
| Prompt hash | `fnv1a64:4916127a304439fb` (instructions + schema; the same for both tiers) |
| Tier A | task = rule name, description, point meanings, parameter values (prose only) |
| Tier B | tier A plus the card's pseudo-code, which is close to a direct specification |

Scrubbing removes every sentence that names any of these:
- a block class: full path, distinctive short name, or the same name as spaced or hyphenated words in any case ("moving average");
- one of the rule's own block ids that has capitals, digits or underscores;
- another library rule's id.

A final check refuses any task that still leaks. Block ids that are plain English words (`persist`, `gap`) count only when written as code. At tier A this removed 36 sentences across test_clean and 132 across all 137 rules.

An earlier run used a case-sensitive scrub that let phrases like "moving average" and references to sibling rules through. Those results were discarded and are not reported here.

## Cost

| Run | Input tokens (cached) | Output tokens (reasoning) | Model calls | Approx. cost |
| --- | --- | --- | --- | --- |
| test_clean A, 3 runs | 559,525 (413,252) | 452,049 (373,887) | 134 | $0.11 |
| test_clean B, 3 runs | 280,665 (252,775) | 207,299 (158,507) | 87 | $0.05 |
| all 137 A, 3 runs | 1,903,046 (1,332,794) | 1,566,337 (1,278,014) | 475 | $0.37 |

Costs use contributor prices with cached input at the cached rate.

## Is it recalling the library?

The library is public, so its rules may be in pretraining data. In the whole-library run, none
of the 396 verified graphs reuses all of the reference graph's block ids, and 116 (29%) use a
different number of blocks than the reference while still passing every vector. This is weak
evidence against verbatim recall. It does not rule out familiarity. Fresh, unpublished rules are
the next step.

## Caveats (keep these beside the numbers)

- **Conformance, not equivalence.** Library vectors are author-written scenarios. Passing them
  means the graph behaves as those scenarios expect, not that it equals the reference
  everywhere.
- **The repair loop saw one pass/fail bit from the scoring vectors.** First-try numbers are
  free of that; after-repair numbers are labelled.
- **Small held-out set.** test_clean has 37 rules and only one over 16 blocks, which is why the
  whole-library breakdown above leads this page.
- **Two control sequences so far.** The plant reset results are 12 runs on one spec; the air
  handler requests are 3 runs on another, and that task spells out every edge.

## Reproduce

```sh
make bench LIB=<open-control-library checkout at 124427b> \
  ARGS="--subset test_clean --tier a --runs 3 --out runs/test_clean-tiera.json"
```

Add `--dry-run` to see the scrubbed tasks (`--show <ID>`) and baselines without calling the
model. Run records (every attempt, reply, finding and token count) are written to `runs/`,
which is not committed.
