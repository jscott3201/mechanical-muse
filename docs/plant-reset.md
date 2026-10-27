# Mechanical Muse: plant reset prep research

Subject: `Buildings.Templates.Plants.Controls.Setpoints.PlantReset` (MBL) as the reference for the CHW and HW plant reset
hero sequence; engine = open-control-engine (OCE); plant = BOPTEST `multizone_office_complex_air`.
Prepared 2026-10-26. All prose below is paraphrase; no ASHRAE Guideline 36 text is reproduced.

## 0. Provenance of what was read

| Item | Source | Pin / hash |
| --- | --- | --- |
| PlantReset.mo | raw.githubusercontent.com/lbl-srg/modelica-buildings, `Buildings/Templates/Plants/Controls/Setpoints/PlantReset.mo` | Identical bytes at `master` (fetched today) and at the engine's vendored MBL commit `a131864e4c4df22ebcd52bb8da439de0087ac365`; sha256 `110f00c5bd12431f9a2bce6094bdba0ee7ab84a596b3a5080d5fa567eb853d6a`, 13,930 bytes. Revision notes in the file: first implementation 2024-03-29, hold logic updated 2024-06-07. |
| MBL validation model | `.../Setpoints/Validation/PlantReset.mo` | same commit; identical to master |
| TrimAndRespond.mo | engine `third_party/modelica-buildings-cdl/Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo` | byte-identical to upstream at the pin (checked) |
| MBL license | `Buildings/legal.html` (vendored in the engine, and upstream) | see section 2 |
| Engine facts | a local clone of open-control-engine (clean, `git status` empty) | `tools/reference-catalog/*`, `docs/cxf-composite-subset.md`, `TESTING.md`, `crates/oce-cxf/tests/*` |
| BOPTEST case | `ibpsa/project1-boptest` raw: `testcases/multizone_office_complex_air/doc/index.html`, `models/wrapped.mo`, `models/MultizoneOfficeComplexAir/{TestCases/TestCase.mo, BaseClasses/ChillerPlant.mo, BaseClasses/BoilerPlant.mo}` | master as of today |

The MBL docs web page (simulationresearch.lbl.gov ... `Buildings_Templates_Plants_Controls_Setpoints.html`) matches the source;
it adds no version history for the block beyond what the `.mo` revision notes say.

---

## 1. What PlantReset does

### 1.1 In plain words

It turns "how many loads are complaining" into two setpoints for one water loop: a supply-temperature setpoint and a
pump differential-pressure (dp) setpoint. One instance serves one set of dp-controlled pumps; the same block serves CHW
and HW (the only difference is which end of the temperature range is the "easy" end).

1. A single dimensionless **reset variable** `r` in [0, 1] is produced by a trim-and-respond (T&R) loop.
2. `r` is mapped through two piecewise-linear curves with a flat segment at each end:
   - **dp curve**: `r` from 0 to `resDp_max` moves dp from `dpSet_min` up to `dpSet_max`; above `resDp_max` dp stays at max.
   - **temperature curve**: `r` from `resTSup_min` to 1 moves the supply temperature setpoint from `TSupSetLim` (the relaxed limit) to `TSup_nominal` (the design value); below `resTSup_min` it stays at the limit.
3. With defaults both knees sit at 0.5, so the two curves hand off to each other: as demand falls from full (`r`=1) the
   **temperature relaxes first** (down to the limit at `r`=0.5) while dp stays at maximum, then **dp relaxes** from
   0.5 to 0. As demand rises, the order reverses: dp is restored first, then the temperature is tightened toward design.
   The two windows may overlap if `resTSup_min` < `resDp_max` (the MBL validation model uses 0.25 and 0.75), in which case both
   setpoints move together over the overlap.
4. Direction depends only on parameters, not on a "CHW" or "HW" switch: CHW has `TSupSetLim` > `TSup_nominal`; HW has
   `TSupSetLim` < `TSup_nominal`. The Line blocks handle either slope.

### 1.2 Interface

| Kind | Name | Type / unit | Meaning |
| --- | --- | --- | --- |
| input | `nReqRes` | Integer | Sum of reset requests from every load this pump set serves |
| input | `u1Ena` | Boolean | Plant (pump set) enabled; drives the T&R device status |
| input | `u1StaPro` | Boolean | A staging process is in progress; drives the T&R hold input |
| output | `TSupSet` | Real, K | Supply temperature setpoint |
| output | `dpSet[nSenDpRem]` | Real array, Pa | One dp setpoint per remote sensor; all move in unison along the same `r` |

| Parameter | Default | Unit | Meaning |
| --- | --- | --- | --- |
| `nSenDpRem` | required | - | Number of remote dp sensors (array size; compile-time) |
| `dpSet_max[nSenDpRem]` | required (each >= 5*6894) | Pa | Max dp setpoint per sensor |
| `dpSet_min` | 5*6894 (= 34,470) | Pa | Min dp the reset may reach (one value for all sensors) |
| `TSup_nominal` | required (>= 273.15) | K | Design supply temperature (reset=1 value) |
| `TSupSetLim` | required (>= 273.15) | K | Relaxed limit (reset <= `resTSup_min` value) |
| `dtHol` | 900 | s | Minimum hold time after a staging event |
| `resDp_max` | 0.5 | 1 | Reset value at which dp reaches its maximum |
| `resTSup_min` | = `resDp_max` | 1 | Reset value at which temperature starts moving |
| `res_init` / `res_min` / `res_max` | 1 / 0 / 1 | 1 | T&R initial, floor, ceiling |
| `dtDel` | 900 | s | Delay before T&R starts after enable |
| `dtRes` | 300 | s | T&R sample period |
| `nReqResIgn` | 2 | - | Number of requests to ignore |
| `tri` | -0.02 | 1 | Trim per sample (must be <= 0) |
| `rsp` | 0.03 | 1 | Respond per excess request (opposite sign to trim) |
| `rsp_max` | 0.07 | 1 | Cap on the response per sample (same sign as respond) |

### 1.3 T&R behaviour, as implemented (paraphrase of the source recurrence)

Verified by reading `TrimAndRespond.mo` and by cross-checking my own reference model against the engine (section 1.6).

- Sample clock: a pulse every `dtRes`, **anchored to simulation time zero**, not to the enable instant.
- While the plant is disabled: output = `res_init` immediately (no delay), internal memory re-armed.
- After enable: output stays at `res_init` until the device has been on for `dtDel + dtRes`. The first trim lands at that instant
  (1,200 s with defaults). By my reading of the block diagram, if that instant is not a tick the first trim still applies at that instant, then continues on the absolute tick grid
  (off-tick effect, ambiguity A8). The reference model encodes this reading; it has not been run through the engine off-tick.
- At each tick, with `R` = sampled request count and `I` = ignored count:
  - `R <= I`: `r` changes by `tri` only.
  - `R > I`: `r` changes by `tri + sign(rsp) * min((R - I) * |rsp|, |rsp_max|)`.
  - result clamped to [`res_min`, `res_max`].
- Hold (here always enabled, `have_hol = true`): the staging flag passes through a true-hold element (hold >= `dtHol` from the rising
  edge, or as long as the flag stays true, whichever is longer). While held, ticks add zero change; the first tick after the hold clears resumes normal updates.
  If the hold clears exactly on a tick, that tick's update applies (release before tick). The engine agrees (section 1.6).
- The T&R block carries two CDL Assert blocks that flag a trim/respond sign mismatch and a respond/cap sign mismatch.
- PlantReset adds only the two Line mappings (limits on, so both curves clamp at their ends) and constants. It has **no rate limit,
  no output clamp to equipment limits, no fault handling, and no multi-loop coordination.** Those are exactly where specs go vague.

### 1.4 Exact blocks instantiated and engine palette check

PlantReset itself instantiates (with `nSenDpRem` = n):

| Instance | Class | Count |
| --- | --- | --- |
| `triRes` | `Buildings.Controls.OBC.ASHRAE.G36.Generic.TrimAndRespond` (`have_hol=true`) | 1 (composite) |
| `resTSup` | `CDL.Reals.Line` | 1 |
| `resDp[n]` | `CDL.Reals.Line` | n (array) |
| `zer`, `one`, `resDpMax`, `resTSupMin`, `TSupSetNom`, `TSupSetMinMax`, `dpSetMin` | `CDL.Reals.Sources.Constant` | 7 |
| `dpSetMax[n]` | `CDL.Reals.Sources.Constant` | n (array) |
| `rep`, `rep1`, `rep2`, `rep3` | `CDL.Routing.RealScalarReplicator` (`nout=n`) | 4 |

TrimAndRespond (44 active blocks in the engine's transcription) adds these elementary classes: `Logical.TrueDelay`, `Reals.GreaterThreshold`,
`Reals.LessThreshold`, `Reals.Switch`, `Reals.Multiply`, `Reals.MultiplyByParameter`, `Reals.Subtract`, `Reals.Add`, `Reals.Min`,
`Reals.Max`, `Reals.Abs`, `Discrete.UnitDelay`, `Discrete.Sampler`, `Conversions.IntegerToReal`, `Logical.And`, `Logical.Not`,
`Logical.Latch`, `Logical.TrueFalseHold`, `Logical.Sources.SampleTrigger`, `Logical.Sources.Constant`, `Utilities.Assert`, plus the
constants and line/replicator above.

**Palette check against `tools/reference-catalog/oce-blocks.registry-manifest.json` (136 entries = 133 CDL classes + 3 lowering pass-throughs):
all 24 distinct elementary classes are present. Missing elementary classes: none.**

Caveats that are not "missing classes" but matter:

1. `Generic.TrimAndRespond` is not an elementary registry class. The engine handles it as a nested composite in explicit CXF
   (resolver rule: unregistered `@type` with `containsBlock` = composite). Its catalog row (`trim_and_respond_have_hol_false`,
   status `supported-runtime-sequence`) covers **only `have_hol=false`**; `have_hol=true` / `TrueFalseHold` is listed under
   `known_deferred_branches`, and the oracle (`tools/golden-gen/.../trim_and_respond_have_hol_false`) does not cover it.
   All three in-tree fixtures that embed it (`trim_and_respond_have_hol_false`, `multizone_vav_supply_temperature`, `multizone_vav_supply_fan`) set `have_hol=false`.
   PlantReset needs `have_hol=true`. See 1.6: it works with a small edit, but has no oracle yet.
2. PlantReset is outside the vendored cone (the cone is "reachable from the 46 G36 fixtures"), so `Setpoints/PlantReset.mo` is not in `third_party/`.
3. Arrays: PlantReset uses array parameters, array block instances and array connectors. The engine's composite subset rejects those
   (`composite/array-parameter`, `composite/array-instance`, `composite/array-connector`), so only a scalarized n=1 form is loadable.
4. `RealScalarReplicator` is width-driven (structural rule); with n=1 it is a pass-through and can simply be dropped in a hand port.

### 1.5 Other MBL reset sequences (answering "elsewhere in MBL")

- `Buildings.Controls.OBC.ASHRAE.PrimarySystem.*` **does not exist at `master`** (`Buildings/Controls/OBC/ASHRAE/package.order` lists only `G36`; the old
  `PrimarySystem/...` raw paths 404). The plant sequences moved to `Buildings.Controls.OBC.ASHRAE.G36.Plants`, whose `package.order` lists only `Chillers`.
- CHW-only equivalents that exist on upstream `master` (both also built on TrimAndRespond with `have_hol=true`). **Correction (2026-10-26):** they are not in the tree at the engine's pin `a131864e`, which has no `G36.Plants` package, so the "disabled ⇒ 0" contrast below cannot be checked against the pinned source:
  - `G36.Plants.Chillers.SetPoints.ChilledWaterPlantReset`: T&R producing a plant reset value; takes a request count, a staging flag, and pump-status array (enable = any CHW pump proven on).
    Note a disagreement inside MBL itself: when disabled this block forces its output to **0**, whereas PlantReset's T&R returns to its **initial value (1)**. A real spec that says "reset to the initial state when off" is
    ambiguous in exactly this way (A1).
  - `G36.Plants.Chillers.SetPoints.ChilledWaterSupply`: maps the reset value to CHW supply temperature and dp setpoints with Line blocks and a "half" knee (0.5); defaults dp min 34,473.8 Pa and plant max CHW supply 288.706 K (60 F).
  - `G36.Plants.Chillers.Pumps.ChilledWater.*`: pump control subsequences.
- **No HW plant reset sequence exists in MBL G36 packages today.** `Templates.Plants.Controls.Setpoints.PlantReset` is the only MBL block whose docs cover HW. That
  makes PlantReset the right single reference for both loops.

### 1.6 Engine probe: does a `have_hol=true` TrimAndRespond load and behave? (a throwaway probe outside the repo)

Method: copied the engine's `trim_and_respond_have_hol_false.jsonld` into a throwaway folder, flipped `have_hol` to true, set `dtHol` 300 (then 360),
added the two connections that exist in the Modelica class only when `have_hol` is true (`uHol -> truHol.u`, `truHol.y -> notHol.u`), and removed the
now-inactive driver connection `fal.y -> notHol.u` (the engine rejects an inactive node that still carries a connection, rule "inactive-conditional-node").
Then drove it through `oce_api::Engine` with a 60 s frame cadence.

Results (the probe code was throwaway and is not in the repository):

- Loads warning-free: 44 blocks, 6 stateful (vs 5 for `have_hol=false`).
- Trace with requests above the ignore count and a staging pulse: output freezes at the first tick inside the hold, resumes at the first tick after release.
- A prototype independent Python reference (written from the `.mo` semantics, not from engine output; superseded by `specs/chw_plant_reset/reference.py`, the validated one) matched the engine on all 41 frames of the
  `have_hol=false` and `have_hol=true` traces, including the hold, with zero differences (1e-9).
- Tie case (hold expires exactly on a sample tick, dtHol=360 with ticks every 120 s): both engine and reference apply that tick's update.
- Takeaway: `have_hol=true` is a **hand-specialization of an existing fixture, not new engine work**. What is missing is an oracle row, not capability.

---

## 2. License

**MBL** (`Buildings/legal.html`): revised 3-clause BSD, plus the US Government notice (DOE funding; Government keeps a paid-up license) and one added
paragraph about "Enhancements". Copyright: Modelica Association, IBPSA, The Regents of the University of California through LBNL, and contributors.
The README says "3-clause BSD". The `.mo` documentation strings are part of the same source files and are covered by the same license.

What it permits (my reading, not legal advice):

| Use | Allowed? | Obligation |
| --- | --- | --- |
| Paraphrase MBL's documentation into our own spec | Yes. Copyright protects expression, not the algorithm or parameter meanings; BSD adds permission on top. | None if wording is our own. A short "reference implementation: MBL PlantReset (BSD-3)" credit is good hygiene and helps the hackathon provenance rule. |
| Copy documentation text or `.mo` code verbatim (or near) into a redistributed artifact | Yes | Keep copyright notice + conditions + disclaimer (conditions 1 and 2). The engine already does this via the vendored `legal.html`. |
| Hand-port PlantReset to CXF as a reference implementation | Yes (derivative work) | Same notice retention in the port's directory or header. Our new code can stay MIT/Apache-2.0; only the ported portion carries the MBL notice. |
| Use the names "Modelica Association", "IBPSA", "LBNL", "UC", "DOE" or contributors to endorse or promote our product | **No** (condition 3) | Factual attribution ("derived from X") is fine; avoid logos, avoid implying endorsement. |
| Commercial use, relicensing our own code | Yes | No copyleft. |
| Publicly releasing "Enhancements" (fixes or upgrades to MBL source) | Allowed, but triggers the added paragraph: a perpetual, royalty-free license to LBNL and everyone to use and sublicense the enhancement | Only matters if we publish edited MBL source. A CXF port is a translation, so low risk; do not publish edited `.mo` files unless that grant is intended. |

Related licenses, same pattern: **BOPTEST** is BSD-style with the same "Enhancements" paragraph (IBPSA); **modelica-json** is BSD-3 (UC/LBNL, no endorsement clause beyond the names).
Neither blocks use of their docs or output.

Two cautions:

1. MBL's license cannot grant rights to ASHRAE's text. Some MBL docs mirror or cite Guideline 36 sections (e.g. the older CHW blocks cite section numbers). We
   write our spec from the implementation's behavior in our own words, as already planned; do not paste MBL's doc tables that restate G36 wholesale.
2. Hackathon rules: repo public, demo must show what was built in the event, third-party assets must be rights-cleared. Keep `third_party/` (MBL, pre-existing) and
   the PlantReset port + vectors + spec PDF + authoring harness (new) in clearly separate paths with a README that says which is which.

**Verdict: permissive; compatible with a public MIT/Apache-2.0 repo; paraphrasing and reference-implementation use are fine. Retain the notice on any ported/copied portion; no endorsement language.**

---

## 3. Reference-oracle feasibility

### 3.1 What the engine accepts

- It is **not** limited to pre-flattened input. `Engine::load_cxf` accepts nested composites (`S231:containsBlock`, depth <= 64) and flattens them, evaluates
  conditional components against grounded parameters, and permutes ports by declared name (so modelica-json's alphabetical connector order is fine).
- It rejects: array-valued composite parameters, array connectors and array block instances; `redeclare`/`extends`/`constrainedby`/replaceable;
  missing parameter values (every parameter needs an `S231:value`); and any active connection touching an inactive conditional node.
- modelica-json (`-o cxf -m cdl`) emits a **class** translation: parameters without values, array markers (`S231:isArray`, `sizeOfDimensions: "(n)"`),
  and conditional edges. Evidence from the engine's own vendored translations: of 44 documents under `third_party/.../cxf`, only 7 import with zero
  diagnostics and 37 carry at least one error (`crates/oce-cxf/tests/vendored_corpus_expectations/mod.rs`). The vendored `TrimAndRespond.jsonld` alone reports
  1 `conditional-guard-unknown-parameter`, 23 `grounding-failed`, 2 `inactive-conditional-node`. The engine uses them only as a **structural oracle** (graph shape
  check against hand-authored fixtures), never as runtime input.
- The engine's own runtime G36 sequences are hand-authored explicit CXF, with Tier-A goldens from `tools/golden-gen` (Rust, zero dependency on `oce-blocks`) that re-derive
  the math from the `.mo` sources; four OpenModelica (OMC 1.25.1) cases exist but are scoped and Docker-based.

### 3.2 What converting PlantReset via modelica-json would take

1. Install modelica-json (Node >= 20, Java for the parser) and an MBL checkout at the pin. (Could not be reproduced here: `codeload.github.com` is blocked from this sandbox, so this is an estimate, not a run.)
2. Translate PlantReset (it lives in `Buildings.Templates`, which is not strictly CDL-compliant; `-m cdl` may need workarounds, fallback `-m modelica` with a `__cdl(isControls=true)` wrapper).
3. Post-process: bind the four required parameters and choose `nSenDpRem=1`; scalarize `resDp`, `dpSetMax`, `dpSet`, `dpSet_max` and drop array markers; drop or keep n=1 replicators;
   nest the TrimAndRespond child with its modifications (`final have_hol=true`, `dtHol` etc.); prune the inactive-driver connection as in 1.6.
4. Iterate on engine diagnostics until warning-free.
5. Even then it is a **structural** cross-check only. A behavioral golden still needs a simulator run (OpenModelica on the MBL validation model, nSenDpRem=2 and unusual knees 0.25/0.75) or a source-derived re-computation.

Effort estimate: (a) 1.5 to 3 days solo, high variance, yields structure only; add 1 to 2 days if an OMC golden is also wanted. (b) Hand-port generated by a ~150-line script from the
existing TrimAndRespond transcription: 3 to 5 hours to a loading CHW instance, +15 minutes for the HW parameter set, +2 to 3 hours for vectors. (c) Behavioral vectors from an independent reference model: already prototyped and cross-checked (section 6), +1 to 2 hours to wire into the engine's `FrameRun` harness.

### 3.3 Recommendation

**Do (b) + (c); defer (a).**

- (b) Generate `plant_reset_chw.jsonld` and `plant_reset_hw.jsonld` (n=1) from a small script that nests the TrimAndRespond (`have_hol=true`) transcription and adds 2 Lines + constants.
  This is the "answer key" the LLM-authored sequence is compared against and also the baseline controller if the live authoring fails.
- (c) Use the section 6 vectors plus differential comparison: run the LLM-authored CXF and the reference port on the same stimulus and compare trajectories at tick instants; add property
  checks (bounds, direction, ordering). Because the reference model was derived from the `.mo` semantics and agrees with the engine on the TrimAndRespond portion, a mismatch between
  engine and model is a meaningful signal (the same anti-tautology principle the repo uses).
- (a) Optional post-hackathon structural cross-check. It does not reduce demo risk.
- State plainly in the demo notes that the reference port is a transcription of MBL (BSD-3, pre-existing logic) and the vectors/model are new.

---

## 4. Proposed parameters for BOPTEST `multizone_office_complex_air`

### 4.1 What the case docs and baseline model say (facts)

| Item | Value | Source |
| --- | --- | --- |
| Plant overwrite inputs | `oveChiWatSys_TWSet_u` 278.15 to 288.15 K; `oveChiWatSys_dpSet_u` 0 to 19,130,000 Pa; `oveHotWatSys_TWSet_u` 291.15 to 353.15 K; `oveHotWatSys_dpSet_u` 0 to 19,130,000 Pa (each with an `_activate` flag) | doc IO list, `wrapped.mo` |
| Readbacks | `reaChiWatSys_{TCHWSup,TCHWRet,dp,mCHWTot,...}_y`, `reaHotWatSys_{THWSup,THWRet,dp,mHWTot,...}_y` | doc IO list |
| dp upper bound is a placeholder | 19.13 MPa = 40 x `dP_nominal` (478,250 Pa); it is not a design value | `TestCase.mo` |
| Baseline CHW supply setpoint | 273.15 + 5.56 = **278.71 K** (5.56 C, about 42 F), fixed | `TestCase.mo` (`TCHWSupSet`) |
| Baseline CHW design delta-T | 5.56 K (design return 11.12 C) | `ChillerPlant.mo` (`dTCHW_nominal`) |
| Baseline HW supply setpoint | 273.15 + 80 = **353.15 K** (80 C); equals the BOPTEST upper bound exactly (`273.15+80 == 353.15` in IEEE double, checked) | `TestCase.mo` (`THWSupSet`); docs say boilers hold 80 C |
| Baseline HW design delta-T | 20 K (design return 60 C) | `BoilerPlant.mo` (`dTHW_nominal`) |
| Baseline CHW dp setpoint | 0.5 x 478,250 = **239,125 Pa**, fixed | `TestCase.mo` (`dpChiWatStaSet`) |
| Baseline HW dp setpoint | 0.25 x 478,250 = **119,562.5 Pa**, fixed | `TestCase.mo` (`dpHotWatStaSet`) |
| Pump heads (docs table) | CHW primary 211,000 Pa (constant speed); CHW secondary 478,000 Pa (variable); HW pumps 478,250 Pa | doc equipment table |
| Equipment | 3 chillers (2.6 MW each), 3 towers, 2 CHW secondary pumps; 2 gas boilers (3.3 MW each), 2 HW pumps; 3 AHUs (floor multiplier: mid floor is 10x) and 15 VAV reheat boxes | doc |
| Staging | chillers: 15 min wait, boilers: 30 min wait, internal state machines, **not exposed** | doc |
| AHU supply air setpoint (occupied) | 12.8 C (285.95 K); BOPTEST AHU SAT overwrite range 285.15 to 313.15 K | doc supervisory table, IO list |
| AHU coils | cooling coil on CHW; **no HW coil in the AHU** (electric freeze-protection coil); HW serves VAV reheat only | doc |
| Plant-off logic in baseline | chiller side disabled when outdoor dry-bulb < 283.15 K and unoccupied | `TestCase.mo` |
| Available measurements for requests | per AHU: `reaAHU_TSup_y`, `reaAHU_yCooVal_y`; per zone: `reaZon*_yReheaVal_y`; occupancy `reaAHU_occ_y` | doc IO list |

The doc does not state a CHW design supply temperature in prose; the 5.56 C figure comes from the baseline Modelica source. The doc also states no dp design setpoint; the dp values come from the same source.

### 4.2 Proposed values

Design principle: **at `r`=1 the controller reproduces the baseline exactly**, so every deviation shown in the demo is a reset-driven relaxation and the KPI comparison is clean.

| Parameter | CHW | HW | Rationale |
| --- | --- | --- | --- |
| `nSenDpRem` | 1 | 1 | Docs expose one dp measurement per loop |
| `TSup_nominal` | 278.71 K | 353.15 K | Baseline setpoint; inside BOPTEST window (HW sits exactly on the upper bound, so also clamp the output) |
| `TSupSetLim` | **282.15 K (9 C)** | **343.15 K (70 C)** | See notes below |
| `dpSet_max` | 239,125 Pa | 119,562.5 Pa | Baseline dp; stays well under design pump head 478,250 Pa |
| `dpSet_min` | 95,650 Pa (0.4 x max) | 47,825 Pa (0.4 x max) | Above MBL's 34,470 Pa (5 psi) floor; a deliberately cautious first guess, **to be tuned against a baseline run** (valve authority at the far loads is not documented) |
| `resDp_max`, `resTSup_min` | 0.5, 0.5 | 0.5, 0.5 | Library defaults (relax temperature first, then dp) |
| `res_init`, `res_min`, `res_max` | 1, 0, 1 | 1, 0, 1 | Library defaults |
| `nReqResIgn` | 2 | 2 | Library default; note 3 AHUs can send up to 9 (see 4.4) |
| `tri`, `rsp`, `rsp_max` | -0.02, 0.03, 0.07 | same | Library defaults |
| `dtDel`, `dtRes`, `dtHol` (realistic) | 900, 300, 900 s | same | Library defaults |
| `dtDel`, `dtRes`, `dtHol` (compressed demo) | 300, 60, 300 s | same | First trim at 360 s, temperature at limit at 1,800 s, dp at floor at 3,300 s (reference model); control step must be a divisor of `dtRes` |

Notes:

- **Why CHW limit 9 C, not the 12.8 C "style" ceiling.** 12.8 C = 285.95 K is numerically legal (the BOPTEST window is 278.15 to 288.15 K), but it equals the baseline AHU supply-air setpoint (12.8 C).
  A coil cannot deliver 12.8 C air from 12.8 C water, so the coil valves would saturate, generate requests, and the loop would fight itself. A limit of about 9 C leaves roughly 4 K of approach
  (engineering judgment, to be confirmed in a run). Keep 285.95 K as a **stress variant**; it also makes a good "bad spec" seed (D1 in section 5).
- **HW limit 70 C**: keeps at least 10 K above the 60 C design return and above typical reheat needs; 60 C (333.15 K) is an aggressive alternative. Both are inside 291.15 to 353.15 K.
- Output clamp: add a `Reals.Limiter` (present in the palette) per output set to the BOPTEST window, and for dp to [0, 478,250 Pa] as a design guard, so a bad LLM-authored sequence cannot request 19 MPa.
- Use the exact doubles above; for HW keep `TSup_nominal` = 353.15 exactly, since BOPTEST bounds are inclusive and the baseline value equals the bound.

### 4.3 Wiring glue that PlantReset does not provide (all expressible with palette blocks)

- `nReqRes` CHW: the sum of the 3 AHUs' `yChiWatResReq` (range 0 to 9). Correction: `Integers.MultiSum` is width-driven and not in the authoring palette; use a chain of two `Integers.Add`.
- `u1Ena`: no plant-enable signal is exposed. Options: occupancy flag `floorN_reaAHU_occ_y` OR a flow threshold with `Reals.Hysteresis` on `reaChiWatSys_mCHWTot_y` / `reaHotWatSys_mHWTot_y`; or any plant request > 0 (`Integers.GreaterThreshold`). Flagged ambiguity A1.
- `u1StaPro`: staging is internal and invisible. v1: constant false (hold path still exercised by test vectors). v2: proxy from a step in chiller or boiler power, with `Logical.TrueFalseHold`. Flagged A5.
- **HW request source gap**: the engine's AHU PlantRequests HW branch is built around an AHU hot-water coil, which this building does not have. HW reset requests would have to come from the 15 VAV reheat valve positions
  (`reaZon*_yReheaVal_y`). The engine's vendored cone contains `TerminalUnits/Reheat/Subsequences/Overrides` and `CoolingOnly/*/SystemRequests` but **no Reheat SystemRequests**. Recommendation: make CHW the end-to-end hero
  (AHU PlantRequests -> sum -> PlantReset), and treat HW as a second, smaller authored sequence (count reheat valves above a threshold for a short time, via `Reals.GreaterThreshold` + `Logical.TrueDelay`, summed with chained `Integers.Add`).
  This differs from the brief's statement that both loops get requests from the existing AHU PlantRequests; worth confirming.
- Floor weighting: each AHU counts once, though the mid-floor AHU carries about 10x the load. Equal weighting is the library convention; flag it (A3).
- Control step: BOPTEST scenario starts fall on multiples of 300 s, so the absolute-time sample clock lines up; use a control step of 60 s or 300 s.

---

## 5. Outline for the one-page spec-style PDF

Format: mechanical-spec "sequence of operations" section, our own wording, single page, two columns if needed. Tag deliberate vagueness as `[A#]` (natural ambiguity, typical of real specs) and seeded
inconsistencies as `[D#]` (hidden defects for the ambiguity finder to catch). Tags go in a separate answer-key page, not on the demo page.

**Title block**: "Hydronic Plant Setpoint Reset: Chilled Water and Hot Water" / project: Large Office, 3 floors / revision date. Keep the numbering style (e.g. "23 09 93.x") plausible but generic.

**1. Purpose and scope** (2 bullets)
- The plant reset reduces pumping and generation energy by relaxing supply temperature and loop pressure whenever the connected loads do not need them.
- Applies to the chilled water loop and the hot water loop; each has its own independent reset.

**2. Definitions** (short)
- "Reset request": whole-number demand signal from a served load. "Plant reset value": a 0 to 100 percent number kept by the controller. "Design" supply temperature. "Relaxed limit".
- [A2] "Requests" left undefined: summed or maximum, per air handler or per zone, equal weight or load weight.

**3. Points list** (small table)
- Inputs: summed reset requests; plant status; staging indicator. Outputs: supply temperature setpoint; loop pressure setpoint. Units stated in SI with one imperial value in parentheses.
- [A1] "Plant status": says "when the plant is running" without naming the proof (schedule, pump status, flow).
- [A5] "Staging indicator": assumes the controller can see staging; this site's staging is internal to the plant.

**4. Sequence** (numbered clauses, the core)
- 4.1 Enable: reset runs only while the plant is on; when off, the reset value returns to 100 percent (full-demand state). [A1]
- 4.2 Start-up delay: no reset for a defined time after start so loads can settle. "A suitable delay" rather than a number. [A4] (the number lives in the parameter table; the finder should notice the clause and table disagree if D2 is seeded)
- 4.3 Periodic evaluation: every few minutes the controller looks at the request total and nudges the reset value: down by a small step if requests are at or below an allowance, up by a larger, capped step proportional to the excess otherwise. Exact formula omitted. [A6]
- 4.4 Hold on staging: while the plant adds or removes equipment, freeze the reset value for a time. [A5] (which clock? "until stable" is undefined)
- 4.5 Mapping to setpoints: at full demand the temperature is at design and pressure at maximum; as demand falls temperature relaxes toward its limit first, then pressure falls to its minimum. [A7] ("first" is not defined as a number range; the overlap is unstated)
- 4.6 Limits: setpoints shall never leave the equipment's allowable range. [A9] ("allowable" by whom: controller, equipment, simulator?)
- 4.7 Multiple pressure sensors: setpoints move together. [A10] (proportional or equal absolute?)
- 4.8 Failure behavior: on loss of request data, "revert to a safe state". [A11]

**5. Adjustable parameters** (table with CHW/HW columns; values from section 4.2)
- Include a deliberately mixed unit presentation (psi and kPa) for the pressure rows. [A12]

**6. Acceptance tests** (3 one-liners)
- Hold the request total at zero for two hours; confirm temperature at its limit and pressure falling. Force a high request total; confirm recovery. Command a staging event; confirm no change during the hold.

**Seeded defects (answer key only)**

| ID | What the PDF says | Why it is wrong | Catching test |
| --- | --- | --- | --- |
| D1 | CHW supply reset range "6 C to 14 C" while the AHU supply-air setpoint table says 12.8 C | Water at 14 C cannot make 12.8 C air; also exceeds sensible coil approach | Property check on temperature limit vs SAT; vector T9 stress |
| D2 | Clause 4.2 says delay "15 minutes" but the table gives 30 minutes (or the clause says trim, table says respond sign) | Inconsistent numbers | Parameter-table lint; vector T1 timing |
| D3 | "Respond step shall be smaller than the trim step" | Reverses the intended asymmetry (respond must dominate; cap exists to limit it) | Vectors T3/T4 |
| D4 | dp maximum stated as "19 MPa" copied from the test-case input range | Placeholder bound, not a design value | Bounds guard T10 |

Natural ambiguity inventory (for the finder's checklist): A1 enable definition; A2 request definition; A3 weighting between loads of very different size; A4 start-up delay number; A5 staging/hold definition and clock; A6 formula omitted (trim/respond semantics, who clamps); A7 knee/overlap definition; A8 sample clock anchor (enable instant vs absolute time; first trim lands off-tick if enable is off-tick); A9 limit authority; A10 multi-sensor semantics; A11 failure behavior; A12 unit mixing (psi/kPa, F/C: 42 F = 5.56 C, 48 F = 8.9 C).

---

## 6. Behavioral test scenarios (vectors)

Numbers below came from the prototype reference model of section 1.6 (not in the repository; the committed vectors come from `specs/chw_plant_reset/reference.py`); its T&R portion matched the engine to 1e-9. Parameters: realistic set
(900/300/900, I=2, -0.02/+0.03/0.07), CHW and HW values from 4.2. `r` = reset value. Times in seconds from t=0 with the plant enabled at t=0 unless stated. All scenarios use tick-aligned times (multiples of 300 s).

Reference mapping (static): CHW `r`=0, 0.25, 0.5, 0.75, 1.0 gives T = 282.15, 282.15, 282.15, 280.43, 278.71 K and dp = 95,650, 167,387.5, 239,125, 239,125, 239,125 Pa.
HW gives T = 343.15, 343.15, 343.15, 348.15, 353.15 K and dp = 47,825, 83,693.75, 119,562.5, 119,562.5, 119,562.5 Pa.

| # | Stimulus over time | Expected | Checks |
| --- | --- | --- | --- |
| T1 | Enable at 0; requests = 0 throughout | `r`=1 for t < 1,200; `r`=0.98 at 1,200, 0.96 at 1,500, one -0.02 per 300 s; `r`=0.50 at 8,400 (CHW T = 282.15 K, HW T = 343.15 K, dp still max); `r`=0 at 15,900 (dp = min); stays 0 afterward | Start delay = dtDel + dtRes, trim cadence, order: temperature first, dp second, floor clamp |
| T2 | Enable at 0; requests = 3 throughout | `r` stays exactly 1.0 for all t (the +0.01 net response is clamped at `res_max`); CHW T = 278.71 K, dp = 239,125 Pa; HW T = 353.15 K, dp = 119,562.5 Pa | Ceiling clamp at initial value; baseline-equivalence at `r`=1 |
| T3 | Start from `r`=0.52 (at 8,100 s of T1), then hold requests at R in {0, 1, 2} | Per tick: -0.02 each (no response when R <= 2) | Ignore allowance |
| T4 | Same start, R in {3, 4, 5, 10} | Per tick: +0.01, +0.04, +0.05, +0.05 (cap at +0.07 respond minus 0.02 trim) | Proportional response and cap |
| T5 | T1 until `r` hits the floor (15,900), requests = 0 until 16,200, then R=5 held | `r` = 0.05 at 16,200, +0.05 per tick, 0.50 at 18,900 (CHW dp back to 239,125 Pa while T still 282.15 K), 0.55 at 19,200 (T = 281.806 K), 1.0 at 21,900 (T = 278.71 K) | Recovery path: dp restored first, then temperature tightened; ceiling clamp |
| T5b | T1 until 8,400, then R=5 | `r` = 0.57 at 8,400, +0.05 per tick, 1.0 at 11,100; dp already at max throughout | Recovery from the temperature-relaxed region |
| T6 | T1; staging flag true for [3,000, 3,600) | `r` frozen at 0.88 through 3,600; hold lasts until 3,900 (rising edge + 900 s, longer than the 600 s pulse); update resumes at 3,900 (0.86); compared with no-hold run, three trims lost (0.06 gap) | Hold = longer of dtHol and flag duration; release-before-tick |
| T7 | T1; staging flag true for [3,000, 4,800) | Frozen at 0.88 until 4,800; first update at 4,800 (0.86) | Long staging overrides dtHol |
| T8 | T1; enable false for [9,000, 9,900) | `r` = 1.0 from 9,000 (setpoints snap to design/max immediately); stays 1.0 until 11,100; then 0.98 at 11,100, 0.96 at 11,400 (delay restarts at re-enable) | Disable reset, re-arm delay |
| T9 | Static sweep of `r` at 0, 0.25, 0.5, 0.75, 1 (inject via test harness on the mapping subgraph), plus overlap variant `resDp_max`=0.75, `resTSup_min`=0.25 | Table above; overlap CHW at `r`=0.5: T = 281.003 K, dp = 191,300 Pa; direction: CHW T falls as `r` rises, HW T rises as `r` rises; dp non-decreasing in `r` | Mapping, knees, sign by loop type |
| T10 | 40,000 s of zero requests, then 40,000 s of R=10 | `r` never < 0 or > 1; T within BOPTEST window (CHW 278.15 to 288.15, HW 291.15 to 353.15); dp within [dp_min, dp_max] and never above 478,250 Pa | Clamps, unit sanity, placeholder-bound guard |
| T11 | Same stimulus with control steps 30 s, 60 s, 300 s | Identical `r` at every 300 s tick (verified in the reference model) | Step-size invariance |
| T12 | Replay T1 + T6 twice; also stress variant CHW limit 285.95 K | Bit-identical traces across replays; stress variant stays inside the window, flags D1 in the finder | Determinism; spec-defect detection |

Notes for the harness:

- Tolerance: 1e-9 on `r`; setpoints are exact products of `r` and constants, so compare to ~1e-9 relative.
- Avoid or separately mark vectors where enable or hold edges fall off the 300 s grid (e.g. enabling at 9,960: the step that appears at 11,160 is a preview of the next update, not an extra one; 11,400 is still 0.98 and 0.96 arrives at 11,700, which matches the library's Dymola validation). They are valid but
  encode the "clock anchored to time zero" choice (A8) and should be an adjudicated vector, not a default one.
- Expected values were not produced by the engine under test (anti-tautology); keep the model in a separate language/path from the CXF and compare, never bless engine output.

---

## 7. Risks and open items

1. `have_hol=true` TrimAndRespond has no oracle row in the engine's catalog; adding one (or at minimum a local note in the demo repo) is part of the port work.
2. Hot-water request source is not available from the existing AHU PlantRequests for this building (no AHU HW coil); decide scope for HW.
3. `dpSet_min` and the CHW/HW limits are engineering first guesses; one baseline BOPTEST run (peak cool day, typical cool day) should confirm no comfort regression before the demo.
4. BOPTEST does not expose staging or a plant-enable signal; `u1Ena` and `u1StaPro` are design decisions (A1, A5).
5. modelica-json run not performed (blocked fetch); option (a) numbers are estimates.

## Appendix: files

The prototype reference model, its scenario script and the engine probe were throwaway files
outside the repository and are not committed. The validated reference and the vectors used by
the tests are in `specs/chw_plant_reset/` (`reference.py`, `vectors.json`). Raw sources were
fetched from the locations in section 0.
