---
name: mm-benchmark-hygiene
description: "Guards the fault-rule benchmark: fixed split, answer-leak scrubbing, rule-level scoring, baselines, run records. Use when building benchmark prompts, running models or writing benchmark numbers."
license: MIT OR Apache-2.0
metadata:
  short-description: "Benchmark split, scrubbing, scoring, run records"
---
Rebuild each open-control-library rule (test data, `124427b`) from a scrubbed spec, then replay its vectors. Depth: `docs/benchmark-split.md` sections 3-5.

**Split** (never re-roll, re-sort or hand-pick)
- `docs/benchmark-split.json`: seed 42, 43 `test`, 94 `train`. Headline subset `test_clean_no_param_identical_train_twin` (37); generalization claims use `test_strict_no_skeleton_train_twin` (24). Fix subset and spec tier before the first run; print both beside every number.
- Few-shot examples only from `few_shot_exemplars` (8, all train). A test rule's card, CXF or vectors never enter a prompt, fixture or repair message.

**Spec scrubbing** (never pass a raw card)
- Keep `name`, `points`, `outputs` (name, meaning), `params` (name, default, unit, description), `Description`. Tier A stops there; tier B adds the `Detection Logic` equation fence (nearly a direct spec: label it "pseudo-code to graph").
- Drop everything else, notably `params.*.cxf`, `diagram.svg`, `Deviations`, `Notes`, `verified`.
- A scrub test fails a spec containing a catalog class name, an instance id of the rule's own graph or a `cxf` key. List failing rules; do not hand-edit quietly.
- Explain secondary outputs (`y...Ok` flags): the interface is locked by name and they carry about 40% of assertions. Put `delayOnInit = true` in the system prompt or emitter (`mm-block-graph-cxf`).

**Scoring** (replay is our code in `mm-bridge`: `mm-oce-engine-host`)
- Headline: rule-level all-scenario pass. Secondary: scenario pass rate. Count load failures and interface mismatches separately.
- Baselines beside every figure, recomputed per subset. Whole library: constant-false passes 720 of 1,760 scenarios (40.9%), constant-true 30.
- First try and after repair are separate numbers with the attempt budget. If repair shows library vectors, label it "repaired against the scoring vectors" or score on a hidden second set.
- First replay every reference rule: cards were verified at `ENGINE_PIN` `41e997f`, our judge is engine `4063202b`. List failures; drop them from the denominator.

**Run record** (JSON beside each result; no keys)
- Model id, effort, temperature (default 1.0, no documented seed: run each rule 3+ times, report spread), prompt and schema hashes, exemplar ids, subset, tier, engine rev, library commit, attempt budget, tokens.
- `muse-spark-1.3-contributor` only here (public data, 150 RPM, no `max` effort; it opts into training use, `mm-meta-model-api`).

**Claims**
- The library is public, so possibly in pretraining data: add 5 to 10 fresh, unpublished rules.
- Write "passes X of Y library test vectors" with baseline and model id. "Spec-derived" only if literally true (library scenarios are author-written). Vectors show conformance, not equivalence (`mm-hackathon-boundary`).
