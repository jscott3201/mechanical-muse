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
- **Fault rules only.** Control sequences (resets, PID loops, hold timers) are a separate track.

## Reproduce

```sh
make bench LIB=<open-control-library checkout at 124427b> \
  ARGS="--subset test_clean --tier a --runs 3 --out runs/test_clean-tiera.json"
```

Add `--dry-run` to see the scrubbed tasks (`--show <ID>`) and baselines without calling the
model. Run records (every attempt, reply, finding and token count) are written to `runs/`,
which is not committed.
