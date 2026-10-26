---
name: mm-web-live-views
description: "Guides web/ live views: SSE stream hook, trends, block-graph viewer, points table, KPIs, degraded states. Use when building or fixing React views fed by mm-server events or adding shadcn components."
license: MIT OR Apache-2.0
metadata:
  short-description: "web/: SSE hook, trends, graph viewer, honest states"
---
Vite + React + TypeScript `strict` + shadcn/ui; the browser talks only to `mm-server`. Fetching lives in hooks. Package manager and Rust-to-TS generator are pending in `docs/DECISIONS.md`.

**Types and ids**
- Payload types are generated from the Rust DTOs; never hand-write a mirror or use `any`. Switch exhaustively on the generated event union (planned envelope `run_id`, `seq`, `sim_time_s`, `kind`: check the union first).
- Key points, nodes and series by CXF `@id`; derive display names in one helper. Data is K and Pa: convert for display only, in one helper.

**Stream** (`EventSource`, one hook per run)
- Close the source in the effect cleanup (StrictMode mounts twice). Reconnect with `Last-Event-ID` = last `seq`; on a gap or new `run_id`, refetch a snapshot.
- Bounded ring buffers per series, one render per animation frame. Never drop or coalesce refusal, degraded, receipt or error events: keep them in a separate log.
- The Vite dev proxy must not buffer or compress `text/event-stream`. `EventSource` sends no headers: no token in the URL.

**Honest states** (icon plus text, never color alone)
- Authoring: drafting, repair attempt n of N, verifying, red with the diagnostic `code()` and subject, green with content id and "passes X of Y". Running: live, stale (age since last event, UI wall clock only), degraded (runtime holds last good setpoints or reverted to baseline, `mm-boptest-runtime`), stopped.
- A missing or non-evaluable value reads "no data", never 0 or last-known as live. Label derived signals "derived" and floor 2 flows and power "x10 multiplier" beside floors 1 and 3.

**Views**
- Graph viewer: read-only, nodes by `@id`, layout cached by content id, diagnostics highlighted by subject, plus a table alternative. Drafts come from block-graph JSON, loaded graphs from the topology DTO; virtualize past 250 nodes.
- Points table: about 295 data points (204 measurements, 91 overwrite inputs); filter by group, floor, zone; virtualize.
- Trend x axis is BOPTEST simulation time. KPIs (`ener_tot`, `cost_tot`, `emis_tot`, `pele_tot`, `pgas_tot`, `tdis_tot`, `idis_tot`) sit beside a baseline run of the same scenario and say BOPTEST computes them.

**Rules**
- Render model text as text: no `dangerouslySetInnerHTML`. Only `VITE_*` reaches the bundle and it is public: no key, ever.
- Add shadcn components with its CLI into `web/src/components/ui`; wrap, never restyle in place. A new component or UI library needs its README row in the same change (`mm-hackathon-boundary`).
- Tests: typecheck, Vitest on the hook with a mock SSE server and scrubbed recorded events, one smoke test of the main flow. Demo mode replays recorded events through the same path (`mm-demo-prep`).
