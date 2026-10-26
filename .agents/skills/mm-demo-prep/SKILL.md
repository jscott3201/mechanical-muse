---
name: mm-demo-prep
description: "Guides the 3-minute demo video and live final: scenes, fallback ladder, recording hygiene, key dates. Use when scripting, rehearsing, recording, freezing or submitting the demo."
license: MIT OR Apache-2.0
metadata:
  short-description: "Demo video, fallback ladder, dates, submission"
---
Scoring: stage 1 pass/fail (theme fit, real Meta model use built in the event); stage 2, 0-10 each: Impact 30%, Use of Meta AI tooling 25%, Technical depth and execution 20%, Demo 25% (works, holds up live, video). The three-minute video is judged beside the repo and the live product; the top 5 per track go to a live final. Scenes: `references/storyboard.md`.

**Dates** (ET)
- Mon Nov 2, 12:00: if the authored plant-reset graph has no green receipt, make a hand-built block graph the hero (graph JSON through the emitter, never hand-written CXF; vectors green) and say so on screen.
- Tue Nov 3, 12:00: feature freeze. After it: fixes, recordings, docs.
- Wed Nov 4, 15:00: submit (portal closes 21:00; the gap is the buffer). Then post on X at once and keep the link (Most Viral bonus counts X engagement through Nov 8).
- Mon Nov 9, 12:00: closing ceremony and live final. The product must start on demand from a clean checkout.

**Video rules**
- First 15 s: name what is new (bridge, emitter, agent, runtime, BACnet mirror, web app, harness) and what is called (engine, library, rusty-bacnet, BOPTEST). Event-built work only (`mm-hackathon-boundary`).
- Show a real red-to-green repair: a draft failing a vector or load check with its diagnostic `code()`, then the green receipt. Use a recorded run where it happened; never stage one.
- Show the closed loop beside baseline KPIs, the refused-frame degraded path, and the benchmark figure with baseline, subset and model id (`mm-benchmark-hygiene`).
- Meta tooling: show only what ran (Muse Spark through the Model API with structured output; name the build harnesses honestly, Muse Code first if it carried the work). Mention Glimmer or open weights only if a run exists; confirm the Modal endpoint details first.
- Under 3:00 (aim 2:50). Format and hosting: check the portal.

**Fallback ladder** (label every rung in the UI and the script; never call 2 to 4 live)
1. Live: Model API, BOPTEST, `mm-server`.
2. Recorded model: scrubbed fixtures replayed through `ModelClient`; real engine and BOPTEST.
3. Recorded run: stored events replayed over the same SSE path, badge "recorded" (`mm-web-live-views`).
4. Screen recording of rung 1 or 2.

**Prep**
- Demo mode is configuration, not code branches: fixed fixtures, fixed BOPTEST scenario and start, images pulled, containers warm (an idle BOPTEST test stops after 900 s: start it just before), model spend alert set.
- Rehearse the final from a clean checkout with the README commands (`docker compose up -d`, `cargo run -p mm-server`, web build).
- Screen hygiene: no keys, `.env`, absolute paths, usernames, emails or session ids visible; review the finished video before upload.
- Submission: repo public, README tables current, secrets and path grep clean, fmt, clippy and tests green, everyone who worked on it listed.
