# Demo script (three minutes)

A plan for the video and the live final, written against what runs today (2026-10-27). Each
scene names its fallback rung; a scene that is not live says "recorded" on screen and aloud.
Numbers are from `docs/benchmark-results.md` and `docs/runtime-results.md`; re-check them
before recording. Spoken lines are guides, not a teleprompter.

## Fallback ladder

| Rung | Model | Plant | How |
| --- | --- | --- | --- |
| 1 live | Muse Spark via the Meta Model API | BOPTEST on Modal | `make boptest-modal`, then `MM_BOPTEST_URL=… MM_BACNET_ADDR=0.0.0.0:47808 make server-live`, web dev server |
| 2 recorded model | recorded replies (`fixtures/recordings/`), labelled "recorded" | BOPTEST on Modal, live | same server; choose "Recorded" in the authoring panel |
| 3 recorded run | — | recorded run events (`fixtures/replays/`), labelled "replay" | `make server`, "Replay" in the run form |
| 4 | screen recording of rung 1 or 2 | | made during rehearsal |

Decide the rung per scene the day before recording. Never call rung 2 to 4 "live".

## Scenes

| Time | On screen | Say (guide) | Rung |
| --- | --- | --- | --- |
| 0:00–0:15 | Title, then the README's two tables | "The muse proposes; the engine checks." Built this week: the authoring agent, graph emitter, engine bridge, BOPTEST runtime, BACnet mirror, web app and benchmark harness. Used, not built here: open-control-engine, open-control-library, rusty-bacnet and BOPTEST. | — |
| 0:15–0:35 | `specs/chw_plant_reset/spec.md` beside a chilled-water trend | Control sequences are written as prose, then hand-programmed and seldom tested before they run. This one resets a chiller plant's water temperature and pressure from air handler requests. | — |
| 0:35–1:15 | Authoring panel: Muse Spark drafts; attempt 1 fails test scenarios (or the engine refuses a loop, with its code); repair; green receipt with content id and "10 of 10" | Spark writes a block graph with structured output; a deterministic engine replays our test vectors and says exactly what is wrong; Spark repairs. Nothing unverified can reach the building. | 1, else 2 |
| 1:15–1:30 | Graph view of the verified draft, then the reference | The draft is a graph of the engine's own blocks, drawn from the receipt; a different graph from the hand-built one, same behaviour on the vectors. | 1 |
| 1:30–2:05 | Run form: the verified draft, day 185 noon; trends, setpoints moving, KPIs beside the baseline run | The runtime re-checks the draft, then drives BOPTEST's large office every 120 s. Over 10 hours: whole-building energy −4.7 %, chiller energy −14 %, thermal discomfort +2 %, all computed by BOPTEST against its own controllers. | 1 (Modal), else 3 |
| 2:05–2:25 | Fault switch: floor 1 supply air temperature replaced by NaN for 5 steps; degraded badge, setpoints handed back, recovery | A labelled test fault. The runtime refuses the bad frame, hands the setpoints back to BOPTEST's own controllers, and resumes when data returns. The model never writes to the plant. | 1, else 3 (`day186-midnight-fault-demo`) |
| 2:25–2:40 | A BACnet client (e.g. YABE) reading device 599 during the run; a write refused | The same points as a read-only BACnet device for the building's existing tools. | 1 |
| 2:40–2:52 | Benchmark table | On 37 held-out fault rules from the open-control-library, described in prose only, Muse Spark (`muse-spark-1.3-contributor`, medium effort) verifies 35 or 36 within three attempts, in each of three runs, without ever seeing the test vectors. A constant-false graph verifies none. | — |
| 2:52–3:00 | Repo URL; Muse Spark via the Meta Model API; built with Muse Code and Claude Code | What is next, one line. | — |

Cut order when over time: BACnet to a still; the graph-view scene into the authoring scene;
the benchmark to one spoken sentence. Never cut the repair, the closed loop or the provenance
opening.

## Claims check before recording

- "checks", "verified against test vectors", "passes X of Y": yes. "Proven", "correct",
  "compliant", "first", "field-tested": no.
- "Uses" or "calls" for the engine, library, rusty-bacnet and BOPTEST.
- KPIs: one window, one season; say "on this 10-hour summer window".
- BACnet: no conformance claim; the vendor id is a placeholder.
- No ASHRAE text on screen; the spec on screen is ours.

## Gaps to close before the Nov 3 freeze

1. ~~KPIs beside a baseline run in the web app~~ Done: the run view sets a run's KPIs beside a
   baseline run of the same window (on the server, or the recorded
   `fixtures/replays/day185-noon-baseline.jsonl`, `ener_tot` 0.3255).
2. A recorded model session (rung 2) from the current prompt, so the recorded fallback shows
   today's behaviour.
3. One manual BACnet read with a desktop client during a live Modal run. (A raw-packet check over
   UDP 47808 during a Modal baseline run passed: I-Am 599, live readings reliable, sequence
   outputs no sensor in a baseline run, a write refused with service-request-denied.)
4. A rehearsal from a clean checkout following the README's "Run it" steps, timed.

## Capture checklist

- Clean checkout, fixed window size, one theme, text readable at 720p.
- Start BOPTEST on Modal about 10 minutes before (initialize takes about 3 minutes; an idle
  test stops after 15). Stop the Modal app afterwards (`modal app stop`).
- Nothing on screen that is a key, `.env`, the Modal URL, an absolute path, a username or an
  email. Watch the exported file once end to end before uploading.
