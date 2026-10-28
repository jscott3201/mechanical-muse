# Demo script (three minutes)

A plan for the video and the live final, written against what runs today (2026-10-28). Each
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
revision, then a second building: a heat pump house on spot prices. The participant guide asks
for a three-minute video that shows only work built during the event, so the cut runs 2:56
(2:42 without the optional fault scene) and opens by naming what is new.

| Time | On screen | Say (guide) | Rung |
| --- | --- | --- | --- |
| 0:00–0:17 | Title card (Push the Frontier; Muse Spark via the Meta Model API), then the README's pipeline diagram with two labelled columns: built during the hackathon, and existing open source we call | Mechanical Muse uses Muse Spark, through the Meta Model API, to turn a written building control sequence into a verified controller. Everything shown was built during this hackathon; the engine, BOPTEST and the BACnet stack are existing open-source projects we call. | — |
| 0:17–0:26 | The sample sheet (`fixtures/sequences/ahu-sat-reset-sample.png`) | Sequences arrive as sheets like this. Each building's are programmed by hand and rarely tested against the sheet before they run. | — |
| 0:26–0:42 | Bring your own → "Use the sample sequence sheet" → the task, what the sheet leaves open, proposed scenarios as timelines; accept | Spark reads the photo into a precise task, lists what the sheet leaves open instead of guessing, and proposes test scenarios. The engineer accepts them; only then do they become the gate. | 2 (recorded), or 1 sped up and said so |
| 0:42–0:55 | Draft revision A: 7 of 7 first try, content id. Building tab: Spark's proposed points, the server's checks, confirm | Each draft is replayed on the engine: seven of seven, with a content id. Spark proposes which building points to use; the server checks units and limits, and the engineer confirms. | 2, or 1 |
| 0:55–1:07 | Revision A on day 143 (replay at 10×): setpoint flat at 55 °F, override latched | On BOPTEST's office it ran safely and did nothing: the sheet's humidity limits keep its override on. We found that in simulation, not in a building. | 3 (`day143-morning-your-sequence-rev-a`) |
| 1:07–1:41 | Revision B: read, accept as `ahu1_sat_rev_b`, draft. Attempt 1 red, 4 of 6; attempt 2 green, 6 of 6; Explain the fix; Try to break it (4 proposals, all pass) | The engineer revises the sheet; same gate. Spark's first draft is red: it sends sixty-five degrees inside the start-up hold. The failing scenarios go back, and the second draft passes six of six. Explain the fix says what changed, in plain words. Then Spark tries to break its own verified draft with four new edge cases; the engine replays them, and all four pass. | 2 (recorded) |
| 1:41–1:56 | Revision B (10×): the reset moves 55→65 °F; tiles beside the baseline | Revision B on the same spring morning: thirty-seven percent less energy than BOPTEST's own controller, with less discomfort. On days it wasn't tuned on, a quarter to a half less in spring and fall, nothing in summer. | 3 (`…-rev-b`), else 1 |
| 1:56–2:22 | Runs → Home → `home-day108-sequence` (10×): peak-price power card 1.17 → 0.87 kW, fleet line, rebound note | Same gate, second building: a heat pump house on spot electricity prices. The sequence preheats before the priciest hours and coasts through them. Over two simulated weeks, power in those hours drops twenty-six percent and the bill eight percent, with less discomfort. Utilities add thousands of homes together; the fleet line is an illustration, and we show the rebound too. | 3 (`home-day108-sequence`) |
| 2:22–2:36 (optional) | Blank-reading replay at 4× (setpoints handed back), then the sensor-bias replay at 10× priced against the same run without it | A blank sensor is refused, and control goes back to BOPTEST. A sensor drifting seven degrees Fahrenheit is worse, because it's believed: about twelve dollars and forty-four kilograms of carbon in two hours. | 3 (`day186-midnight-fault-demo`, `day185-noon-fault-sensor-bias`) |
| 2:36–2:49 | README results table (muse-spark-1.3-contributor, 3 runs); a short Muse Code clip calling the MCP tools; BACnet mirror still | On a hundred thirty-seven fault rules, Spark verifies at least one thirty after repair, never seeing the tests. Muse Code helped build this, and coding agents get the same checks over MCP. | — |
| 2:49–2:56 | Repo and docs URLs; Muse Spark via the Meta Model API | Muse Spark proposes, the engine checks, and only verified logic runs. Links below. | — |

Cut order when over time: the fault scene; the Try to break it sentence; the binding sentence;
the summer clause. Never cut the opening line on what was built this week, the repair, the
revision A → B contrast, or the Muse Code line. Never go over 3:00.

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
- open-control-engine and rusty-bacnet predate the event: "existing", never "our new engine".
- Muse Code "helped build" the repo; the README names Claude Code too.
- Home: "simulated", "BOPTEST's own controller", the fleet line is an illustration, mention
  the rebound (highest 15-minute draw +16.4 %).
- US English: °F first, "fall" not "autumn".
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
- Captions burned in; no music or footage without the rights.
- Video link opens without sign-in (check in a private window); repo public; everyone who
  worked on it listed on the submission. Submit by Nov 4, 3 PM ET (portal closes 9 PM ET), then
  post on X.
- Start BOPTEST on Modal about 10 minutes before (initialize takes about 3 minutes; an idle
  test stops after 15). Stop the Modal app afterwards (`modal app stop`).
- Nothing on screen that is a key, `.env`, the Modal URL, an absolute path, a username or an
  email. Watch the exported file once end to end before uploading.
