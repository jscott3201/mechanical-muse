# Three-minute storyboard

Plan, not fact: adjust each scene to what actually runs by Nov 3 and cut a scene rather than fake it. Spoken text in our own words; no standard text on screen.

## Scenes

| Time | On screen | Say | Scores |
|---|---|---|---|
| 0:00-0:15 | Title, then the README provenance split | "The muse proposes. The engine proves." New: authoring agent, graph emitter, engine bridge, runtime, BACnet mirror, web app, benchmark harness. Called, not built here: open-control-engine, open-control-library, rusty-bacnet, BOPTEST. | Demo, rules |
| 0:15-0:40 | A plain-language sequence (chilled-water plant reset, paraphrased) next to a building trend | The problem: sequences are written in prose, then hand-programmed and rarely proven before they run. Who benefits: controls engineers and commissioning teams. | Impact |
| 0:40-1:20 | Agent view: Muse Spark drafts a block graph; the engine load or a vector fails (diagnostic `code()`, failing vector); repair attempt; green receipt with content id | Structured output keeps drafts well-formed; the deterministic engine decides if they are right. Show the real red, then the real green. | Meta tooling, Technical depth |
| 1:20-2:05 | Live BOPTEST run: floor trends, setpoints moving, KPIs beside the baseline-only run | Only the verified graph reaches the building emulator. Say what the KPIs compare, the scenario, and that BOPTEST computes them. | Demo, Impact |
| 2:05-2:25 | Inject a refused frame or a NaN sensor: degraded badge, setpoints held, then recovery | The model never writes to the plant; the runtime owns the fail-safe. | Technical depth |
| 2:25-2:45 | Benchmark result with baseline; BACnet point list from the mirror | "Passes X of Y library test vectors" for the named model, subset and tier, constant-false baseline beside it. Mirror points are simulated devices, no conformance claim. | Technical depth, Impact |
| 2:45-2:55 | Repo URL, tools used (Muse Spark, Muse Code, Model API; Glimmer only if run) | What comes next, kept short. | Meta tooling |

## Cut order when over time
1. Shorten the BACnet segment to a still of the point list.
2. Merge the problem scene into the opening line.
3. Drop the benchmark scene to one sentence over the red-to-green repair.
Never cut the repair, the closed loop or the provenance split.

## Capture checklist
- Record from a clean checkout, fixed window size, dark or light theme chosen once, text readable at 720p.
- One take per scene, pre-warmed containers, fixed BOPTEST scenario, demo-mode fixtures for the model step.
- Every non-live segment carries the "recorded" badge on screen and a spoken "recorded run".
- Re-read the script against `mm-hackathon-boundary`: "uses" and "calls" for engine, library, rusty-bacnet and BOPTEST; no "first", "proven", "G36 compliant", "field-tested".
- Watch the exported file once end to end for keys, paths, usernames and emails before uploading.
- Rehearse the live final separately: same stack, cold start, the fallback ladder rung chosen in advance.
