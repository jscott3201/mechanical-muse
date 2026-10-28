# Demo script (three minutes)

A plan for the video and the live final, written against what runs today (2026-10-27). Each
scene names its fallback rung; a scene that is not live says "recorded" on screen and aloud.
Numbers are from `docs/benchmark-results.md` and `docs/runtime-results.md`; re-check them
before recording. Spoken lines are guides, not a teleprompter.

## Fallback ladder

| Rung | Model | Plant | How |
| --- | --- | --- | --- |
| 1 live | Muse Spark via the Meta Model API | BOPTEST on Modal | `make boptest-modal`, then `MM_BOPTEST_URL=… MM_BACNET_ADDR=0.0.0.0:47808 make demo-live` |
| 2 recorded model | recorded replies (`fixtures/recordings/`), labelled "recorded" | BOPTEST on Modal, live | same; choose "Recorded run" in the authoring panel |
| 3 recorded run | — | recorded run events (`fixtures/replays/`), labelled "replay" | `make demo` (no key, no BOPTEST), "Replay" in the run form |
| 4 | screen recording of rung 1 or 2 | | made during rehearsal |

Decide the rung per scene the day before recording. Never call rung 2 to 4 "live".

## Scenes

The story is one engineer's sequence, from a photo of the sheet to a run on the building, then a
revision. Our own chilled-water reset carries the closed loop in the background and gets one beat.

| Time | On screen | Say (guide) | Rung |
| --- | --- | --- | --- |
| 0:00–0:15 | Title, then the README's pipeline diagram (built in the event vs used unmodified) | "The muse proposes; the engine checks." Built this week: the authoring agent, graph emitter, engine bridge, BOPTEST runtime, BACnet mirror, MCP server, web app and benchmark harness. Used, not built here: open-control-engine, open-control-library, rusty-bacnet and BOPTEST. | — |
| 0:15–0:30 | The sample sheet (`fixtures/sequences/ahu-sat-reset-sample.png`) | Sequences arrive as a sheet like this. A contractor programs them by hand for each building, and they are seldom tested before they run. | — |
| 0:30–0:55 | Bring your own → the photo → Spark's reading: task, open questions, proposed scenarios as timelines; edit one value, accept | Muse Spark reads the photo with structured output and lists what the sheet leaves open instead of guessing. Its test scenarios are proposals; the engineer accepts them, and only then do they become the gate. | 1 (sped up, say so), else 2 |
| 0:55–1:25 | Draft: attempt 1 red, 4 of 6 scenarios (it sends 65 °F inside the 10-minute start-up hold); the failing scenario goes back; attempt 2 green, 6 of 6, content id | A deterministic engine replays the scenarios and says exactly where the draft is wrong. Spark repairs it. Nothing unverified can reach the building. | 2 (revision B's real red-to-green), or 1 |
| 1:25–1:40 | Bind: Spark's proposed points, the server's checks, confirm; a glance at the graph | Spark also proposes which building points feed each input and which setpoint each output writes, within limits; the server checks every point and unit; the engineer confirms. | 2, or 1 |
| 1:40–2:00 | Runs: revision A on day 143 — the setpoint flat at 55 °F, the override latched | It ran, safely, and did nothing: the sheet's humidity thresholds keep its override on in this building. The tool found that before anyone deployed it. | 3 (`day143-morning-your-sequence-rev-a`) |
| 2:00–2:20 | Revision B (70 / 65 %): re-read, re-verify, re-run; the reset moves 55→65 °F; KPIs beside the baseline | The engineer edits the sheet and runs it through the same gate. On this window: energy −37 % and less discomfort against BOPTEST's own controller. On days it was not tuned on: about a quarter to a half less HVAC energy in spring and autumn, mostly reheat gas, and nothing in summer. Say that. | 3 (`…-rev-b`), else 1 |
| 2:20–2:35 | Fault switch: a sensor replaced by NaN; degraded badge, setpoints handed back, recovery | A labelled test fault. The runtime refuses the bad frame, hands the setpoints back to BOPTEST's own controllers, and resumes when data returns. | 3 (`day186-midnight-fault-demo`), else 1 |
| 2:35–2:50 | Benchmark table; one line on the MCP server; BACnet mirror as a still | On 137 open fault rules described in prose, Spark verifies 130 to 132 after repair, never seeing the test vectors; a constant graph verifies none. Coding agents get the same checks over MCP, and live runs appear as a read-only BACnet device. | — |
| 2:50–3:00 | Repo URL; Muse Spark via the Meta Model API | What is next, one line. | — |

Cut order when over time: the BACnet still; the binding beat into the draft beat; the benchmark to
one spoken sentence. Never cut the repair, the revision A → B contrast, or the provenance opening.

## Claims check before recording

- "checks", "verified against test vectors", "passes X of Y": yes. "Proven", "correct",
  "compliant", "first", "field-tested": no.
- "Uses" or "calls" for the engine, library, rusty-bacnet and BOPTEST.
- KPIs: name the window ("on this 8-hour spring morning"); revision B's thresholds were chosen
  after revision A failed on that same day, so quote the held-out range beside it.
- BACnet: no conformance claim; the vendor id is a placeholder.
- No ASHRAE text on screen; the spec on screen is ours.

## Gaps to close before the Nov 3 freeze

1. ~~KPIs beside a baseline run in the web app~~ Done: the run view sets a run's KPIs beside a
   baseline run of the same window (on the server, or the recorded
   `fixtures/replays/day185-noon-baseline.jsonl`, `ener_tot` 0.3255).
2. Rung 2 today plays back a recorded session with a real red-to-green (the engine refuses a
   loop, then 8 of 10, then 10 of 10), from before the loop rule was in the prompt; it is
   labelled "recorded". The server now keeps every live session's replies
   (`runs/recordings/<session>.json`); to switch the fallback to a session on the current
   prompt, copy one to `fixtures/recordings/chw_plant_reset.json` and update the tests that
   pin its outcomes.
3. One manual BACnet read with a desktop client during a live Modal run. (A raw-packet check over
   UDP 47808 during a Modal baseline run passed: I-Am 599, live readings reliable, sequence
   outputs no sensor in a baseline run, a write refused with service-request-denied.)
4. ~~A rehearsal from a clean checkout~~ Done 2026-10-28 with `make demo` (rung 3, recorded model):
   overview, recorded Spark session (refused as a loop, 8 of 10, 10 of 10, verified), run this
   draft, replays of the live-draft and fault runs, KPI tiles beside the baseline. Fixed on the
   way: live model offered without a key, no path from a verified draft to a run, the fault not
   marked on the charts, repeated axis labels on narrow ranges.

## Capture checklist

- Clean checkout, fixed window size, one theme, text readable at 720p.
- Start BOPTEST on Modal about 10 minutes before (initialize takes about 3 minutes; an idle
  test stops after 15). Stop the Modal app afterwards (`modal app stop`).
- Nothing on screen that is a key, `.env`, the Modal URL, an absolute path, a username or an
  email. Watch the exported file once end to end before uploading.
