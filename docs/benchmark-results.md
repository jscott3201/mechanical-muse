# Benchmark results

How well Muse Spark rebuilds open-control-library fault rules from a scrubbed task, scored by
replaying each rule's library test vectors through the pinned engine. Method, split and hygiene
rules: `docs/benchmark-split.md` and `.agents/skills/mm-benchmark-hygiene/SKILL.md`. Harness:
`crates/mm-bench`.

## Run of 2026-10-26

| Setting | Value |
| --- | --- |
| Model | `muse-spark-1.3-contributor` (public library data only), effort `medium`, `max_output_tokens` 32,000 |
| Attempts | up to 3 per rule; repair feedback `validator-only` (validator and load problems in full; a replay failure only as "behavior does not match", with no scenario names, counts or values) |
| Exemplars | none (zero-shot) |
| Runs | 3 independent runs per rule (no documented seed) |
| Judge | our replay in `mm-bridge` on open-control-engine `4063202b`; open-control-library `124427b` |
| Prompt hash | `fnv1a64:4916127a304439fb` (instructions + schema; identical for both tiers) |
| Tier A | task = rule name, description, point meanings, parameter values (prose only) |
| Tier B | tier A plus the card's pseudo-code, which is close to a direct specification |

Scrubbing removed every sentence that named a block class or one of the rule's own block ids
(4 sentences at tier A). Seven test_clean rules were skipped at tier B because their pseudo-code
names a class or block id (CHW-0009, ERV-0002, HW-0001, SYS-0010, TOWER-0003, VAV-0005,
VFD-0003); none were skipped at tier A.

### Rule-level all-scenario pass (headline), per run

| Subset | Tier | Rules | First try | After repair (≤ 3 attempts) | Scenarios (best attempt) | Constant-false baseline |
| --- | --- | --- | --- | --- | --- | --- |
| test_clean | A | 37 | 34, 34, 34 | 35, 36, 37 | 475, 477, 478 of 478 | 0 rules; 204 of 478 scenarios |
| test_clean | B | 30 | 30, 30, 30 | 30, 30, 30 | 391 of 391 (each run) | 0 rules; 163 of 391 scenarios |
| test_strict | A | 24 | 21, 22, 21 | 22, 23, 24 | 314, 316, 317 of 317 | 0 rules; 126 of 317 scenarios |
| test_strict | B | 20 | 20, 20, 20 | 20, 20, 20 | 263 of 263 (each run) | 0 rules; 104 of 263 scenarios |

Constant-true passes 0 rules in every subset (6 of 478, 5 of 391, 3 of 317, 3 of 263 scenarios).
Every first-try failure was a behavior mismatch found by replay; no draft failed validation,
the interface lock, or engine load. The rules that failed at least once: FPB-0004 and HP-0006
(tier A).

In words: at tier A, Muse Spark's first draft passes all library test vectors for 34 of 37
test_clean rules in each of three runs; with up to two repair turns that see only validator
output and a pass/fail bit, 35 to 37 of 37.

### Cost and size

| Tier | Input tokens | Output tokens (reasoning) | Model calls |
| --- | --- | --- | --- |
| A, 3 runs | 496,756 | 433,159 (361,329) | 102 rules on attempt 1, 3 needed 2, 6 needed 3 |
| B, 3 runs | 292,908 | 210,461 (160,668) | all on attempt 1 |

At contributor prices that is roughly $0.14 (tier A) and $0.07 (tier B).

### Is it recalling the library?

The library is public, so its rules may be in pretraining data. Two checks on the 198 verified
graphs: none reuses all of the reference graph's block ids, and at tier A 29 of 108 use a
different number of blocks than the reference (16 fewer, 13 more) while still passing every
vector. Reused ids are generic (`notStatus`, `absErr`). That argues against verbatim recall,
but it does not rule out familiarity; a set of fresh, unpublished rules is still planned.

## Caveats (keep these beside the numbers)

- **Conformance, not equivalence.** Library vectors are author-written scenarios. Passing them
  means the graph behaves as those scenarios expect, not that it equals the reference
  everywhere.
- **Tier B is close to a direct spec.** Its 100% says the model translates pseudo-code into
  CDL blocks reliably; tier A is the harder test.
- **The repair loop saw a pass/fail bit from the scoring vectors.** First-try numbers are free
  of that; after-repair numbers are labelled.
- **Small test set.** test_clean has 37 rules and is light on large ones (one rule over 16
  blocks).

## Reproduce

```sh
make bench LIB=<open-control-library checkout at 124427b> \
  ARGS="--subset test_clean --tier a --runs 3 --out runs/test_clean-tiera.json"
```

Add `--dry-run` to see the scrubbed tasks (`--show <ID>`) and baselines without calling the
model. Run records (every attempt, reply, finding and token count) are written to `runs/`,
which is not committed.
