# Bring your own sequence (the workbench)

An engineer gives Muse Spark a written sequence of operations (pasted text, a PDF, or a photo of
the sheet). Spark turns it into our task format and proposes test scenarios. The engineer reviews
and accepts the scenarios. From then on the sequence is handled like our own specs: Spark drafts
a block graph, the engine replays the accepted scenarios, and failures go back to Spark until
every scenario passes. A verified draft downloads as CXF.

## Who decides what is right

The expected values in the scenarios are model output. They become the verification gate only
after the engineer accepts them. The web app shows each scenario as a timeline (input steps, and
the windows where an output is checked with the expected value) next to an editable table. Any
scenario can be turned off or corrected.

The checks (`mm_agent::check_task`) refuse scenarios that cannot run fairly:
- unknown points;
- an input missing from a scenario;
- steps that do not start at 0 or fall off the clock grid;
- values of the wrong type;
- check windows outside the run.

The checks cannot tell whether an expected value is right. A sequence accepted from the workbench
has no hand-built reference graph, and the UI says so. If a draft keeps failing one scenario, the
UI asks the engineer to check that scenario too.

Spark is also told to list what the document leaves open (exact-threshold behavior, start-up
values, what happens on a lost sensor) as `open_questions`, rather than decide silently. It is
told to avoid checking instants the document does not settle.

## API
- `POST /api/workbench/read` `{text, attachments: [{name, mime, data_base64}]}` returns `ReadView`:
  the draft, problems, and source (`live`, or `recorded` when a committed reading has the same
  digest). One reading runs at a time. The request limit is 20 MB, with up to 4 PDFs or images.
- `POST /api/workbench/accept` `{draft, key}` checks the edited draft and registers it as a
  sequence (422 with the problems if it fails; it can never take a built-in spec's name). It is
  saved under `<runs>/workbench/` and restored at start-up.
- `GET /api/workbench` lists accepted sequences. They also answer at `/api/specs/{name}`,
  authoring, explanations and drafts. Asking for their graph gives 404 `no-reference-graph`.
- `GET /api/drafts/{content_id}/cxf` returns a verified draft's CXF, emitted again from its graph.

## Running it on the building
An accepted sequence runs on BOPTEST once it is bound to the building. Binding decides:
- which reading feeds each input:
  - a measurement, for a real input;
  - a measurement above a threshold, for a boolean input;
  - or a constant;
- which setpoint each output writes, and within what limits. An output can also be display-only.

Points that exist on every floor are written with `{floor}`, and the binding names the floors it
runs on.

Muse Spark proposes the binding from BOPTEST's point list (`docs/boptest-points.csv`), with notes
on stand-ins such as occupancy used as an AHU enable. `crate::bind::check` then checks it:
- every point exists and is the right kind (a measurement, or a `_u` setpoint);
- every input is bound once, with a feed that fits its type;
- only real outputs are written;
- limits are finite, ordered and inside the setpoint's own range;
- no point is written twice.

When the checks refuse the proposal, Spark gets one retry with the findings listed. The engineer
edits the binding and confirms it on the sequence's Building tab.

A run can then include a verified draft of the sequence (the run form's "Your sequences"). The
run records the binding it used. As for every sequence, the draft's scenarios are replayed again
before it loads (AGENTS.md hard rule 5), and the runtime's fail-safe applies. It loads one
instance per floor (`<spec>_floor<N>`). The run view charts each output sent beside the
measurement that shows its result, and each measured input.

- `POST /api/workbench/{name}/binding/propose`: Spark's proposal (`live`, or `recorded` from
  `fixtures/bindings/` when the task matches and there is no key), with the checks' findings.
- `POST /api/workbench/{name}/binding` `{binding}`: confirm (422 with the findings if refused).
  It is saved under `runs/workbench/bindings/` and restored at start-up.
- `GET /api/workbench/{name}/binding`, `GET /api/workbench/bindings`.
- `POST /api/runs` with `drafts: {<name>: <content id>}` runs it; it is refused while the
  sequence is unbound.

### First runs on BOPTEST (2026-10-27)
The sample sequence ran on all three floors, beside our chilled-water reset and air handler
requests, in four windows on BOPTEST on Modal. Every run completed with no fail-safe event, and
every step it sent 55 °F (12.78 °C), as the sheet says for each condition it met:
- **Day 185, noon to 22:00.** Outdoor air was 27–33 °C, above the reset band. 55 °F is 0.1 K
  colder than BOPTEST's own 12.88 °C, so against our sequences alone energy savings fell from
  4.7 % to 2.3 % and thermal discomfort rose (+13.4 % vs +2.0 % against the baseline).
- **Day 110.** Return humidity was 62–64 %, so the dehumidification override was on for 237 of
  240 steps.
- **Day 140.** A weekend: unoccupied, so the AHU counts as off.
- **Day 143, from 06:00.** Occupied, and outdoor air passed through the band (14–25 °C). But
  return humidity started at 64 % and stayed above 60 % for the first hour, which turned the
  override on. It then held at 57–60 %, above the 55 % release, so the override stayed on all
  morning (237 of 240 steps per floor; day 110 latched the same way).

So the drafted logic does what the sheet says, and running it closed-loop shows that the sheet's
humidity thresholds keep its reset from ever acting in this building. A reviewer would want to
know that before deploying it. The day 185 and day 143 runs are replays in `fixtures/replays/`
(labelled "with your sequence").

### Revision B: the engineer adjusts the sheet (2026-10-27)
`fixtures/sequences/ahu-sat-reset-sample-v2.md` raises the dehumidification thresholds to suit
this building: on above 70 %, off below 65 %. Everything else is unchanged. It went through the
whole product again, live:
- **Read** from the PNG in 47 s, with no check problems; the reference grader agrees at 108 of
  108 checked instants (`--rev b`).
- **Accepted** under a new name.
- **Drafted.** Attempt 1 passed 4 of 6 scenarios: it sent 65 °F inside the 10-minute start-up
  hold. Attempt 2, after the failing scenarios went back to Spark, passed all 6.
- **Bound** by a clean proposal.
- **Run** over the same day 143 window as revision A (06:00, 8 h). Revision B, our sequences
  alone, and a baseline over the window are all replays in `fixtures/replays/day143-morning-*`.

| Day 143, 06:00, 8 h, against baseline | Energy | Cost | Emissions | Thermal discomfort |
| --- | --- | --- | --- | --- |
| Our sequences alone | −3.3 % | −5.3 % | −4.3 % | +0.5 % |
| + revision A | −0.7 % | −3.4 % | −2.1 % | +6.7 % |
| + revision B | **−37.3 %** | **−27.9 %** | **−32.5 %** | **−10.7 %** |

With revision B the setpoint followed the outdoor-air reset from 12.8 to 18.3 °C (55 to 65 °F).
The override came on only once, for 5 steps on floor 3 around 08:40, when that floor's return
air reached 70 %; it released below 65 %, as the sheet says. There was no fail-safe event and no clamp. This is one mild
8-hour window on one day, not a rate; BOPTEST computes the KPIs.

### Revision B on days it was not tuned on (2026-10-27)

Revision B's thresholds were chosen after revision A failed on day 143, so day 143 alone says
little. The same three runs (BOPTEST's baseline, our sequences alone, our sequences plus
revision B) went over five more windows on Modal, using the recorded reading, draft and binding.
Energy, and thermal discomfort, against the baseline over each window:

| Window | Our sequences alone | + revision B | Thermal discomfort, + revision B |
| --- | --- | --- | --- |
| Day 110 (April), 06:00, 8 h | −4.6 % | **−54.4 %** | −48.5 % |
| Day 143 (May), 06:00, 8 h (tuned on) | −3.3 % | −37.3 % | −10.7 % |
| Day 150 (late May), 00:00, 24 h | −3.2 % | **−23.4 %** | −66.0 % |
| Day 157 (June), 06:00, 8 h | −4.8 % | −4.9 % | −9.4 % |
| Day 199 (July), 06:00, 8 h | −5.6 % | −3.3 % | **+7.8 %** |
| Day 262 (September), 06:00, 8 h | −2.4 % | **−55.4 %** | −76.2 % |

The split by source (`docs/emissions.md`) shows where it comes from. On the cool days boiler gas,
which feeds the zone reheat coils, falls by half to two thirds (Scope 1 on day 110: 2,587 to
844 kgCO2e; day 262: 1,994 to 573), because a supply air 4 to 5 K warmer needs far less
reheating at the zones. Electricity moves much less. In summer the outdoor air is above the
reset band, so the sheet's reset sits at 55 °F, like the baseline, and adds nothing; on day 199
it did slightly worse than our sequences alone. So the sheet's value depends on the season:
large in spring and autumn, none in summer. These are single windows in simulation, and BOPTEST
computes the KPIs. The run records are kept locally, not committed.

Offline, revision B plays back under the name Spark gives it: recorded sessions and bindings
are found by a digest of the task's content (`task_key`), not only by name.

These revision A runs used Spark's first binding. Its lower limit of 285.93 K sat 0.002 K above 55 °F, so
the replays log a `clamped` event on every write. The binding prompt now asks for limits that
include every value the sequence sends. Three new live proposals all chose 285.9–291.5 K, and
the recorded binding in `fixtures/bindings/` is one of them; the offline test requires zero
clamps.

Writing `floorN_TSupAirSet` also changes what our air handler requests sequence reads as the
supply air setpoint, because BOPTEST reports the override back. That is intended: the requests
compare the measured supply air with the setpoint actually in force.

## Try to break it
A verified draft has passed only the scenarios Spark proposed and the engineer accepted. Those
can miss edges. Once the sequence has a verified draft, the engineer can ask Spark to try to break
it (the button sits with the verified drafts on the sequence's Graph tab):
- **What Spark sees.** The written sequence as accepted (description, points, parameters,
  clock), the accepted scenarios and the open questions. It never sees the draft, so its
  expectations come from the sequence, not from what one draft happens to do. The sequence's name
  is left out too, so a revision accepted under a new name asks the same question.
- **What it proposes.** Three to five new scenarios, each with a one-line reason naming the edge:
  a value exactly at a threshold, the last tick of a hold, an interrupted delay, two inputs
  changing on the same tick, a point the open questions leave unsettled. Same schema as the
  reader's scenarios, medium effort, one call (`mm_agent::attack`).
- **The checks.** Each proposal is checked on its own with `check_task`. One that cannot run
  fairly is dropped, with the reason listed. A name that clashes with an accepted scenario gets a
  suffix. At most five are kept.
- **The replay.** The server replays every proposal against the verified draft the engineer
  picked, one fresh engine each, and reports pass, or the first failing check as authoring does
  (output, instant, expected and got). This is only a test: nothing runs on the building, and the
  sequence's own scenarios and receipts do not change.
- **Who decides.** A failing proposal means the draft or the proposal is wrong. The UI says so,
  labels everything "Proposed by Muse Spark: tests, not verdicts", and selects nothing. The
  engineer picks the proposals they agree with and accepts them as a new revision (`<name>_r2`,
  then `_r3`) through `/api/workbench/accept`, so every accept check applies. The revision is
  then drafted, verified and repaired like any sequence. The original stays as it was.

`POST /api/workbench/{name}/attack` `{draft: <content id>}` returns `AttackView`:
- `key`: a digest of what was sent to the model;
- `model` and `source` (`live`, or `recorded`);
- the accepted `sequence`;
- each proposal with its `result`;
- the `problems` (the proposals the checks dropped).

It answers 404 for an unknown sequence and 400 for one of our own specs or a content id that is
not a verified draft of this sequence. It answers 409 `model-busy` while another model call for the workbench is under way, and 503 `no-model` without a key and without a
recorded reply for that key. Replies are cached by key: in memory, under `<runs>/attacks/`, and
read-only from `fixtures/attacks/`, which is served labelled recorded. Replays are never cached.

### First live attack (2026-10-28)
`make live-attack` asked Spark to break revision B's verified draft. It used the committed reading
and drafting session; only the attack was live (58 s, `fixtures/attacks/`). Spark proposed four
scenarios, and the checks dropped none:

| Proposal | Edge it probes | Draft |
| --- | --- | --- |
| `oat_exact_thresholds` | Outdoor air exactly at 60 °F, then exactly at 70 °F (the reset's ends) | passes |
| `humidity_exact_boundaries` | Return humidity exactly 70 % (must not latch), then exactly 65 % (must not release) | passes |
| `dehum_entry_continuity_reset` | A short dip below 70 % inside the 5-minute entry delay must restart it | passes |
| `simultaneous_oat_dehum_exit` | The override releases on the same tick that outdoor air moves to mid-band | passes |

The draft held on all four. Our hand-written reference for revision B
(`fixtures/sequences/ahu-sat-reset-reference.py --rev b`) also agrees with every one of the 68
instants the proposals check. That holds when "above 70 %" is read as strict, as the sheet
says. Read inclusively, 4 instants of `humidity_exact_boundaries` differ. That is exactly the
edge the proposal probes, and the draft and Spark agree on the strict reading. One attack on one
sequence, not a rate.

## Without a key
`fixtures/sequences/ahu-sat-reset-sample.{md,pdf,png}` is a one-page sheet we wrote for this demo
(an AHU supply air temperature reset in °F, with a start-up hold and a dehumidification override).
`fixtures/readings/` holds Spark's live reading of the PNG. `fixtures/recordings/ahu1_sat_stpt.json`
holds Spark's live drafting session for the accepted reading. So `make demo` plays the whole flow
back, labelled recorded, and every check still runs. For revision B accepted as `ahu1_sat_rev_b`,
`fixtures/attacks/` also plays back Spark's attempt to break its verified draft.

## Results so far (one document, written by us; a small sample)
- **Pasted text** (`make live-read`): one model call; 5 points, 11 parameters, 8 open questions
  and 6 scenarios. The °F values were converted to K correctly. The checks found no problems.
- **PNG of the sheet** (through the server): read in 47 s; 7 scenarios and 8 open questions.
- **Graded against our own reference**, `fixtures/sequences/ahu-sat-reset-reference.py`. We wrote
  it by hand from the sheet, not from model output; it is independent of Spark, but its authors
  also wrote the sheet. Where the sheet is silent it is run once per reading: a strict or an
  inclusive 60 % threshold, and whether the dehumidification flag can be on while the AHU is off.
  Under each reading on its own, every instant Spark checks agrees:
  - 118 of 118 for the PNG reading, which is committed as `fixtures/readings/` (rerun it with
    `python3 fixtures/sequences/ahu-sat-reset-reference.py fixtures/readings/read_*.json`);
  - 138 of 138 for the text reading, kept in `runs/` and not committed.
- **Drafts:** Spark's draft against its own accepted scenarios was verified on the first attempt
  both times.
