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

Running an accepted sequence on BOPTEST needs a binding from its points to BOPTEST's. That is not
built yet, so the UI does not offer a run for these sequences.

## Without a key
`fixtures/sequences/ahu-sat-reset-sample.{md,pdf,png}` is a one-page sheet we wrote for this demo
(an AHU supply air temperature reset in °F, with a start-up hold and a dehumidification override).
`fixtures/readings/` holds Spark's live reading of the PNG. `fixtures/recordings/ahu1_sat_stpt.json`
holds Spark's live drafting session for the accepted reading. So `make demo` plays the whole flow
back, labelled recorded, and every check still runs.

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
