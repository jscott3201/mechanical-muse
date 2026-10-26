# Benchmark design: cards, vectors, verifier, split, exemplars, risks

Source repo: `open-control-library` (137 fault rules, all `status: verified`, `ENGINE_PIN` `41e997fd`). The split was computed by a prototype script (`make_split.py`, kept in local research notes); machine-readable lists are in `docs/benchmark-split.json`.

## 1. What a rule folder contains

`faults/<family>/<ID>/{card.md, rule.cxf.jsonld, vectors.json, diagram.svg}`. Contract: `SCHEMA.md` ("`card.md` contract", "`vectors.json` contract"). 14 families; sizes in the split table below.

**card.md** = YAML frontmatter + Markdown prose (median 11.4 KB / 205 lines, max 23 KB).
- *Frontmatter* (26 required keys, plus `validation` on 30 cards and `adjudicates` on 6): identity and triage (`id`, `name`, `equipment`, `status`, `phase`, `method`, `severity`, `category`, `confidence`, `source`, `clusters`, `related`, `playbooks`), host contract (`operating_states`, `preconditions`: gating prose, e.g. "host gates evaluation; unmet means NO_EVAL"), the **interface** (`points`: bare canonical point names, median 3, max 5; `outputs`: `{name, description}`, 1 output on 61 cards, 2 on 37, 3 on 27, 4 on 11, 5 on 1), the **thresholds** (`params`: name -> `{default, unit, description, cxf}`; median 3, max 10; units mostly s, degC, %, 1), impact fields, and `verified {engine_rev, content_id, date}`.
- *Thresholds live in frontmatter, not prose.* The equation refers to them by name. 525 of the 728 graph parameter values are exposed through `params`; 203 are not (177 are `TrueDelay.delayOnInit = true`, the rest mostly `pre_u_start` and a few constants).
- *Prose sections*: `Description` (median 0.8 KB: what the fault is and why it matters), `Detection Logic` (median 1.3 KB: a fenced pseudo-code equation on **all 137 cards**, median 6 lines, plus the diagram link and a few paragraphs on timing/strictness), `Possible Diagnoses`, `Energy Impact`, `Emissions Impact`, `Deviations` (largest, median 3.2 KB: every engineering decision against the source reference, e.g. hysteresis, `delayOnInit`), `Notes` (119 of 137).
- A *task spec* (name, points, outputs, params without `cxf`, Description, equation fence) is median 2.4 KB (~590 tokens), max 5.5 KB.

**vectors.json** (`cxf-library/vectors/v1`): `clock {step_s, horizon_s}` and `scenarios[] {name, description, inputs, expect}`.
- `inputs`: every card point appears in every scenario, either a constant (2,965 floats, 892 bools, 66 ints) or a piecewise-constant step list `[{t, value}]` (906).
- `expect[]`: `{output, from_s, to_s, equals}` windows, inclusive, checked at every tick in the window. All 4,463 assertions are boolean `equals`; none uses `tolerance`. 2,661 target `yFault`, 1,802 target secondary outputs (evaluability and sub-condition flags).
- Totals: 1,760 scenarios (median 12 per rule, 4 to 28), median 24 assertions per rule. `step_s` 60 s on 79 rules, 300 s on 46, others 5 to 150 s; horizon median 5,400 s, max 777,600 s (9 days). Whole library = 238,607 ticks. 98 scenarios are tagged "Reference vector" (published vectors); the rest are authored threshold-edge and transient cases. Every rule has at least one scenario asserting `true`.

## 2. `tools/verify` and what "verified" means

`tools/verify` is the Rust crate `cxf-verify` (`src/main.rs` 772 lines, `src/lint.rs`, `src/point_resolution.rs`, `build.rs`), an in-process client of `oce-api`.

Invocation (cwd = library root):

| Command | Effect |
|---|---|
| `cxf-verify --all` | discovers every `faults/*/*/` containing `rule.cxf.jsonld`, prints `discovered N fault dirs`, lints + replays each |
| `cxf-verify <dir>...` | same for listed dirs |
| `cxf-verify --replay-only <dir>...` | replay only, lint skipped; explicit dirs only (rejected with `--all`). Built for generated graphs (`tools/simharness` uses it; `tools/dataset_harness` also runs temporary vectors through `tools/verify`) |
| `cxf-verify --trace-json <dir> <vectors.json>` | prints a `cxf-library/replay-trace/v1` JSON of every boundary output at every tick |

Output per directory: the path; `content_id: cxf:fnv1a128:...` (engine export identity); `LINT ok` or `LINT <error>` (`LINT skipped (generated replay)` with `--replay-only`); then one line per scenario, `PASS name` or `FAIL name -- t=<s>s: <output> = <got> but expected <want> (window <from>..<to>s)` (first violation only); last line `all scenarios passed`. Exit 0 = all pass, 1 = any lint or scenario failure (including `ERROR load_cxf failed: ...`), 2 = usage error. Replay semantics: fresh engine per scenario; inputs staged before the first tick with time >= their `t`; ticks at `k*step_s` for `k = 0..floor(horizon/step)`; assertions at every tick inside `[from_s, to_s]`; Real compares use tolerance 1e-9; inputs and outputs are matched by bare name via a `.name`/`#name` path suffix and an ambiguous match is an error.

**"Verified" in this library** (`SCHEMA.md` "Verification" + `lint.rs`): (1) the rule loads in the pinned engine; (2) every scenario passes at every tick of every assertion window; (3) lint passes: card schema/enums, `id` equals folder, every `points` entry resolves in the family point dictionary, card `points`/`outputs` equal the CXF boundary inputs/outputs exactly, every `params.*.cxf` path resolves to a parameter node, vectors only touch declared points/outputs, scenario names unique, `diagram.svg` exists; (4) `verified.engine_rev` prefixes `ENGINE_PIN`, `verified.content_id` equals the engine's export id, `date` set. It is *executable-spec conformance against author-written scenarios on one engine revision*: no formal equivalence, no external Modelica oracle, and gating/NO_EVAL stays with the host. For a benchmark only (1) and (2) apply (`--replay-only`).

**Environment gotchas.** `build.rs` hard-asserts that `../../../open-control` (a sibling directory of the library clone) is at exactly `ENGINE_PIN` with a clean tracked tree. The engine clone here is named `open-control-engine`, is at `4196a8d` (merge of PR #337), and the pinned `41e997f` is not in the shallow clone; either fetch and check out `41e997f` as `open-control`, or re-pin and re-verify (every `verified.content_id` is tied to the pin). I did not build or run `cxf-verify`.

**Metric design.** A constant-`false` graph passes 720 of 1,760 scenarios (40.9%); constant `true` passes 30 (1.7%). Report **rule-level all-scenario pass** as the headline, scenario pass rate as a secondary, and keep load failure and lint-style interface mismatch separate.

## 3. Stratified held-out split (seed 42, ~30% test)

Algorithm (language-independent, no RNG library): per family, sort IDs ascending by `sha256("42:" + id)`; the first `max(1, floor(0.3*n + 0.5))` are test (a family of one goes to train). Result: **43 test, 94 train** (31.4%). Every family has test and train rows.

| Family | Rules | Test | Train |
|---|---|---|---|
| ahu | 39 | 12 | 27 |
| chw | 9 | 3 | 6 |
| erv | 5 | 2 | 3 |
| fcu | 7 | 2 | 5 |
| fpb | 6 | 2 | 4 |
| hp | 8 | 2 | 6 |
| hw | 12 | 4 | 8 |
| hx | 3 | 1 | 2 |
| pmp | 6 | 2 | 4 |
| rtu | 11 | 3 | 8 |
| sys | 10 | 3 | 7 |
| tower | 6 | 2 | 4 |
| vav | 10 | 3 | 7 |
| vfd | 5 | 2 | 3 |
| **all** | **137** | **43** | **94** |

**TEST (43)**, number only, prefix = family: ahu 0001 0002 0010 0012 0013 0014 0018 0020 0034 0035 0037 0038; chw 0002 0006 0009; erv 0002 0004; fcu 0002 0004; fpb 0002 0004; hp 0003 0006; hw 0001 0002 0006 0007; hx 0003; pmp 0005 0006; rtu 0006 0008 0010; sys 0003 0005 0010; tower 0003 0004; vav 0004 0005 0006; vfd 0001 0003.

**TRAIN (94)**: ahu 0003 0004 0005 0006 0007 0008 0009 0011 0015 0016 0017 0019 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032 0033 0036 0039; chw 0001 0003 0004 0005 0007 0008; erv 0001 0003 0005; fcu 0001 0003 0005 0006 0007; fpb 0001 0003 0005 0006; hp 0001 0002 0004 0005 0007 0008; hw 0003 0004 0005 0008 0009 0010 0011 0012; hx 0001 0002; pmp 0001 0002 0003 0004; rtu 0001 0002 0003 0004 0005 0007 0009 0011; sys 0001 0002 0004 0006 0007 0008 0009; tower 0001 0002 0005 0006; vav 0001 0002 0003 0007 0008 0009 0010; vfd 0002 0004 0005.

Diagnostics: test blocks per rule median 7 (max 19, HP-0006), train median 7 (max 28). Size bins (<=4, 5-9, 10-16, 17+ blocks): test 5/28/9/1, train 20/46/21/7, so **the test set is light on large rules** (only HP-0006, 19 blocks, is 17+; next are VAV-0005 at 14 and CHW-0002/HW-0006 at 13). Methods: test 37 `rule` + 6 `statistical` (AHU-0038, CHW-0002, FPB-0004, HW-0002, HW-0006, PMP-0006); train 76 + 17 + 1 `meta`. 23 of 43 test rules have more than one output. All classes used in test appear in train, but three have thin support there (`Reals.Min` 2 uses, `Logical.Sources.Constant` 2, `Logical.Switch` 1). Two derived subsets (see Risks) are in `benchmark-split.json`: `test_clean` (37, no param-identical twin in train) and `test_strict` (24, no graph-skeleton twin in train).

## 4. Few-shot exemplars (all TRAIN; none has a skeleton twin in TEST)

Chosen greedily for class coverage weighted by library frequency, then adjusted by hand for size spread and to include the only train rules with `Reals.Switch`, `Integers.*` and feedback. Together: 27 of the 33 library classes, **97.8% of library block instances**. Prompt cost: 17.8 KB of wire JSON (~4.4k tokens) plus ~5.8k tokens of task specs, about 10k tokens for all eight (the first five: ~2.1k + ~3.1k = ~5k).

| # | Rule | Blocks / edges | Wire B | Why |
|---|---|---|---|---|
| 1 | AHU-0016 Simultaneous heating and cooling | 4 / 6 | 947 | The canonical idiom: two `GreaterThreshold` -> `And` -> `TrueDelay`. SCHEMA.md names its card as the style exemplar. |
| 2 | VFD-0005 VFD not in remote automatic control | 6 / 13 | 1,760 | Pure Boolean logic (`Not`, 3x `And`, `Or`, `TrueDelay`) with fan-out; no Real signals. |
| 3 | CHW-0004 Chilled water low delta-T | 7 / 11 | 1,688 | Arithmetic with a `Constant` and `MultiplyByParameter`, `Subtract`, a Real-vs-Real `Less`, then `And`/`TrueDelay`. |
| 4 | SYS-0009 Sensor flatline while equipment active | 8 / 12 | 2,029 | Time-sampled stateful logic: `Discrete.Sampler` -> `Subtract` -> `Abs` -> `LessThreshold`, three `TrueDelay`s. |
| 5 | FCU-0001 Excessive operating state changes | 10 / 12 | 2,106 | Event counting: `Integers.Change`, `BooleanToInteger`, `IntegerToReal`, `MovingAverage`, `MultiplyByParameter`, `Logical.Sources.Constant`. Type conversions. |
| 6 | RTU-0002 Evaporator coil fouling (statistical) | 14 / 21 | 2,265 | `Reals.Switch` in a non-CUSUM setting; mode gating via `Integers.Equal`/`GreaterThreshold`/`LessEqualThreshold`, `Divide`. |
| 7 | ERV-0005 Supply/exhaust airflow imbalance | 17 / 31 | 3,414 | Large two-branch analog rule: 2x `Abs`, `Add`, 2x `Greater`, 4x `And`, 2x `TrueDelay`. |
| 8 | VAV-0007 Airflow tracking CUSUM (statistical) | 22 / 37 | 3,550 | Upper complexity end and the only exemplar family with feedback (`UnitDelay` accumulator), `Switch`, `Max`, constants. |

Not covered by the eight: `Logical.Change/Edge/Switch`, `Reals.AddParameter/Min/Multiply` (3 to 7 uses each). Gap to plan for: the library has no PID, `Hysteresis`, `Line`, `Limiter` or `Latch` rules, so control-authoring exemplars must be hand-converted from the G36 fixtures (for example `ahu_supply_air_temp_reset.jsonld`), and their emitter path is untested.

## 5. Risks

**Cards that leak the answer.** The card is both the task statement and a partial solution; build prompts from a scrubbed spec, never the raw card.
- `params.*.cxf` is on every card (463 parameters): `persist.delayTime` gives the instance name and, via the parameter name, the class (`delayTime` -> `TrueDelay`, `t` -> a `*Threshold`). Drop `cxf`.
- `diagram.svg` labels every instance id and (136/137) every class with key parameters. Never include it; the card body links it.
- Prose names blocks: 86/137 cards name at least one class from their own graph (`TrueDelay` in 59 cards, `MovingAverage` 21), only 4 name all; 98 mention `CDL`/`Buildings.Controls`; 99 cite instance ids in backticks; 90 cite parameter names. The `Detection Logic` section names a class in 56 cards (17 of the 43 test cards). Worst: FCU-0001 (5 of 9 classes, 11 instance ids, 4 parameter names), AHU-0022, VAV-0005, SYS-0006, VFD-0002, AHU-0037. 3 of 43 test cards are clean (no class or instance mention). The `Deviations` section also narrates graph choices (79 cards): exclude it, `Notes`, `Energy`, `Emissions`, `Possible Diagnoses`.
- The equation fence is itself a near-direct specification (all 137 cards; on 51 it reuses block ids as variable names, on 19 it contains a class name). A benchmark on it measures pseudo-code -> block graph. For a harder tier, give `Description` + point list only. Decide the tier before reporting numbers.
- Conventions the card does not state: `delayOnInit = true` on every `TrueDelay` (177/177); put it in the system prompt or have the emitter default it.
- Secondary outputs (evaluability flags `y...Ok`, direction flags) carry 40% of all assertions; the spec must state their meaning because the interface is locked by name.

**Near-duplicates across splits.** Using a Weisfeiler-Lehman hash of the port-labelled graph: 101 distinct skeletons (class + wiring, parameter values ignored) among 137 rules; 57 rules share a skeleton with another; the largest group has 8 (`Subtract -> GreaterThreshold -> TrueDelay`: AHU-0005, 0009, 0011, 0012, 0014, 0015, HP-0005, RTU-0009). With parameter values, 9 pairs are functionally identical up to names: AHU-0005/0011, AHU-0007/0013, AHU-0014/0015, CHW-0005/0006, FCU-0002/0003, FCU-0004/0005, FCU-0006/FPB-0001, HP-0004/RTU-0008 (identical card name too), SYS-0001/0002.
- After the stratified split, **19 of 43 test rules have a skeleton twin in train and 6 have a parameter-identical twin**: AHU-0013 (~AHU-0007), AHU-0014 (~AHU-0015), CHW-0006 (~CHW-0005), FCU-0002 (~FCU-0003), FCU-0004 (~FCU-0005), RTU-0008 (~HP-0004). Report headline metrics on `test_clean` (37) and, for a generalization claim, `test_strict` (24); or switch to a skeleton-grouped split. Few-shot exemplars were picked to have no skeleton twin in test.
- Siblings across families (the same fault bound to different point dictionaries) explain most of this; cards say so in `Deviations`.

**Other**
- *Vectors under-specify.* Passing author scenarios does not prove equivalence; a graph can pass and still differ off-vector. Keep vectors hidden at generation time; hold back a second, fresh vector set if budget allows.
- *Public-repo contamination.* The library is on GitHub, so cards and CXF may be in pretraining data; a held-out split does not address that. Consider 5 to 10 freshly authored rules as a never-seen set.
- *Statistical rules* (23, plus 1 `meta`) are 17% of the library and 14% of test, structurally harder (CUSUM, feedback).
- *Pin mismatch* between the clone (`4196a8d`) and `ENGINE_PIN` (`41e997f`): the palette and the verifier must come from the same engine revision as the one that judges the results.
- *Optional extra evidence*: `tools/simharness` (EnergyPlus false-positive sweep, uses `--replay-only`) and `tools/dataset_harness` (external datasets) already replay rule directories through `tools/verify`; 30 cards carry a `validation:` block from such runs.
