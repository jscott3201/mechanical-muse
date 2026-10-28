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
| 0:30–0:50 | Bring your own → "Use the sample sequence sheet" → Spark's reading: task, the eight things the sheet leaves open, proposed scenarios as timelines; accept | Muse Spark reads the photo with structured output and lists what the sheet leaves open instead of guessing. Its test scenarios are proposals; the engineer accepts them, and only then do they become the gate. | 1 (sped up, say so), else 2 |
| 0:50–1:05 | Draft revision A: verified on the first attempt, 7 of 7 scenarios, content id. Bind: Spark's proposed points, the server's checks (limits in K, °C and °F), confirm | Every draft is replayed on a deterministic engine before anything runs. Spark also proposes which building points feed each input and which setpoint each output writes; the server checks every point and unit; the engineer confirms. | 2, or 1 |
| 1:05–1:25 | Runs: revision A on day 143 (replay at 10×) — the setpoint flat at 55 °F, the override latched | It ran, safely, and did nothing: the sheet's humidity thresholds keep its override on in this building. The tool found that before anyone deployed it. | 3 (`day143-morning-your-sequence-rev-a`) |
| 1:25–1:55 | "Revision B of the sample" (70 / 65 %): read, accept (named `ahu1_sat_rev_b`), draft. Attempt 1 red, 4 of 6 (it sends 65 °F inside the 10-minute start-up hold); the failing scenarios go back; attempt 2 green, 6 of 6 | The engineer edits the sheet and it goes through the same gate. The engine says exactly where the draft is wrong; Spark repairs it. Nothing unverified can reach the building. | 2 (revision B's real red-to-green), or 1 |
| 1:55–2:15 | Run revision B (10×): the reset moves 55→65 °F; tiles beside the baseline: energy −37.3 %, cost −27.9 %, discomfort −10.7 % | On this window: energy −37 % and less discomfort against BOPTEST's own controller. On days it was not tuned on: about a quarter to a half less HVAC energy in spring and autumn, mostly reheat gas, and no gain over our own sequences in summer. Say that. | 3 (`…-rev-b`), else 1 |
| 2:15–2:35 | Two faults: the blank-reading replay at 4× (fail-safe note: setpoints handed back), then the sensor-bias replay at 10× — its tiles now compare with the same run without the fault, and the Energy & emissions panel prices it | A blank reading is refused and control goes back to BOPTEST. A reading that drifts 4 K is worse: it is believed, the plant runs colder, and BOPTEST computes the cost, about $12 and 44 kg of CO2 in two hours, a fifth of the ten hours' saving. | 3 (`day186-midnight-fault-demo`, `day185-noon-fault-sensor-bias`), else 1 |
| 2:35–2:50 | The README's results table (the app does not show the benchmark); one line on the MCP server; BACnet mirror as a still | On 137 open fault rules described in prose, Spark (muse-spark-1.3-contributor, 3 runs) verifies 130 to 132 after repair, never seeing the test vectors; a constant graph verifies none. Coding agents get the same checks over MCP, and live runs appear as a read-only BACnet device. | — |
| 2:50–3:00 | Repo URL; Muse Spark via the Meta Model API | What is next, one line. | — |

Cut order when over time: the BACnet still; the binding beat into the draft beat; the benchmark to
one spoken sentence. Never cut the repair (in the revision B beat), the revision A → B contrast, or
the provenance opening.

## Rehearsal timings

Timed on 2026-10-28 with `make demo` (rung 3, recorded model, warm build cache): the server
answered 19 s after start. Waits on screen: reading 0.1 s; revision A draft 2.1 s; binding about
1 s; revision A replay 6.5 s at 10× (15.6 s at 4×); revision B read and draft 4.1 s; revision B
replay 6.5 s at 10×; blank-reading fault 1.8 s at 10× (60 steps: play it at 4×); sensor-bias
replay 8.3 s at 10×. About 30 s of waiting in all, so the three minutes are set by the narration.

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
