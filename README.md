# Mechanical Muse

**The muse proposes. The engine checks.**

Mechanical Muse is an agent built on Meta's Muse Spark that writes building control logic from a written sequence of operations, and lets it run only once it passes its tests. A deterministic control engine checks every draft against test vectors, and only verified logic goes live on a simulated large office building.

Built solo by Justin Scott for the Meta Global AI Developer Hackathon (Indie track), October 26 to November 4, 2026.

> Status: work in progress during the hackathon.

## Run it

Quickest: `make demo`, then open http://127.0.0.1:8080. It builds the web app and serves it from
the API on one address. With no model key and no BOPTEST it still verifies every spec, plays back
a recorded Muse Spark session, recorded BOPTEST runs and recorded explanations, all labelled as
recorded. `make demo-live` adds live Muse Spark from a key in `.env`; set `MM_BOPTEST_URL` for
live runs and `MM_BACNET_ADDR` for the BACnet mirror.

### Run it without toolchains

With only Docker: `docker compose -f compose.demo.yaml up --build` (or `make docker-demo`), then
open http://localhost:8080. Same recorded mode as `make demo`, no key and no BOPTEST needed; the
first build takes several minutes. Add `-f compose.demo.live.yaml` for live Muse Spark from your
own `MODEL_API_KEY` in `.env`. On Modal: `modal serve deploy/modal/mm_demo.py`. Details, live
mode and troubleshooting: [docs/packaging.md](docs/packaging.md).

### Full setup

You need Rust (see `rust-toolchain.toml`), Node (`web/.nvmrc`) and a BOPTEST v0.9.0 checkout
(`git clone --branch v0.9.0 --depth 1 https://github.com/ibpsa/project1-boptest`).

1. BOPTEST, either locally with Docker (`export BOPTEST_DIR=<checkout>; make boptest-up`, slow on
   Apple silicon) or on Modal with native x86 (`make boptest-modal`, see
   [docs/boptest-on-modal.md](docs/boptest-on-modal.md)).
2. The server: `make server` (set `MM_BOPTEST_URL` for Modal). For live Muse Spark authoring, put
   a Meta Model API key in `.env` as `MODEL_API_KEY` and run `make server-live`.
3. The web app: `cd web && npm ci && npm run dev`, then open the printed address.
4. Optional: mirror live runs read-only over BACnet/IP with `MM_BACNET_ADDR=0.0.0.0:47808` on the
   server ([docs/bacnet-mirror.md](docs/bacnet-mirror.md)).

Without BOPTEST or a model key, the web app can still verify the specs, play back the recorded
Muse Spark session (labelled "recorded"), and replay recorded BOPTEST runs (labelled "replay").

## Built during the hackathon

Everything in this repository was written during the event, starting October 26, 2026. The commit history is the record.

| Component | Path | Status |
| --- | --- | --- |
| Engine bridge: load receipts, frame stepping, vector replay | `crates/mm-bridge` | Working offline: replays all 1,760 library scenarios on the pinned engine |
| Block-graph schema and CXF emitter | `crates/mm-graph` | Working offline: validator and emitter round-trip all 137 library rules |
| Muse Spark authoring agent: prompt, Responses client, draft-verify-repair loop | `crates/mm-agent` | Working live against Muse Spark (`docs/model-api.md` section 10); reports progress for live views; plays back recorded runs. With vector feedback, Spark drafts a passing chilled-water plant reset in 2 of 3 benchmark runs, and in 3 of 3 live sessions after the repair message learned to name loops (`docs/benchmark-results.md`) |
| Control sequence specs: our own text, independent reference models, test vectors, hand-built graphs | `specs/` | Four specs, each gated by a test that its reference graph passes every vector and that common mistakes fail: chilled-water plant reset and air handler chilled-water requests (running on BOPTEST); duct static pressure reset and zone static pressure requests (validated and wired into the runtime: the `pressure` run option) |
| Library benchmark harness: scrubbed tasks, baselines, run records | `crates/mm-bench` | First results in `docs/benchmark-results.md` |
| BOPTEST runtime host: supervisory loop, BOPTEST client, fail-safe | `crates/mm-runtime` | Runs the verified AHU requests and CHW plant reset against BOPTEST live; 10-hour comparison with the baseline in `docs/runtime-results.md` |
| HTTP API: specs, typed block graphs, verification receipts | `crates/mm-server` | Working: spec, graph and verify endpoints; authoring sessions (Muse Spark, or a labelled recorded run) streamed as server-sent events, with verified drafts stored with their receipts; BOPTEST run sessions that can run a drafted sequence after replaying its vectors again, or inject a labelled test fault (a blank, stuck or biased reading); a Scope 1 and 2 energy, emissions and cost ledger per run from BOPTEST's KPIs and recorded power readings, and comparisons of runs of the same window ([docs/emissions.md](docs/emissions.md)); labelled replays of recorded runs; run records; generated TypeScript types |
| MCP server for coding agents: specs, palette, validator, CXF emitter, vector replay as tools | `crates/mm-mcp` | Working offline over stdio, on the official MCP Rust SDK (`rmcp`): Muse Code or any MCP client drafts a block graph and our tools check it (passes X of Y scenarios, with the same repair text our agent gets); no model calls of its own ([docs/mcp.md](docs/mcp.md)) |
| BACnet device mirror | `crates/mm-bacnet` | Working: live runs served read-only as one BACnet/IP device (55 objects: BOPTEST readings, sequence outputs, setpoints sent); remote writes refused; tested over UDP ([docs/bacnet-mirror.md](docs/bacnet-mirror.md)) |
| Web app | `web/` | Working: specs and verification receipts; Muse Spark authoring followed attempt by attempt (engine refusals, failing scenarios, the green receipt; recorded runs labelled); start BOPTEST runs on reference or drafted graphs and follow them live (status, receipts, trends, KPIs beside a baseline run of the same window, Scope 1 and 2 energy and emissions with end uses and a test fault's cost against the same run without it, event log); block graph view of reference and drafted graphs, with the blocks a problem names outlined; plain-language explanations written by Muse Spark (labelled; recorded copies for use without a key) of a graph, reference or drafted, whose parts outline their blocks, and of a run's events (faults, handbacks) in an operator's words; replays of recorded runs; bring your own sequence: Muse Spark reads pasted text, a PDF or a photo of a sequence of operations, lists what it leaves open and proposes test scenarios, the engineer reviews and accepts them, then Spark drafts against them, a verified draft downloads as CXF, and once Spark's proposed binding to BOPTEST's points is checked and confirmed it runs on the emulated building beside our own sequences ([docs/workbench.md](docs/workbench.md)) |
| Design notes and API references | `docs/` | Started |
| Coding-agent setup: agent context, project skills, verification hook | `AGENTS.md`, `.agents/skills/`, `.muse/`, `.mcp.json`, `scripts/` | Started |

The application's model calls all go to Meta's models (Muse Spark through the Meta Model API; Muse Glimmer for the open-weights comparison). The repository was built with Meta's Muse Code and with Claude Code as coding tools.

## Pre-existing dependencies (not built during the hackathon)

These are public, open-source projects used unmodified, each pinned to one commit. All but open-control-engine are pinned to commits from before the event. open-control-engine is developed independently in its own public repository; on October 27 we moved our pin to its `development` branch at `fa5cb19`, which includes engine fixes made during the event (some prompted by gaps we reported, listed in `docs/upstream-gaps.md`). Those engine changes are not part of this entry. On the new pin every verdict in our benchmark records is unchanged (`docs/benchmark-results.md`).

| Dependency | Author | License | Pinned commit | Used for |
| --- | --- | --- | --- | --- |
| [open-control-engine](https://github.com/jscott3201/open-control-engine) | Justin Scott | MIT OR Apache-2.0 | `fa5cb19` (`development`, 2026-10-27) | Executing and verifying CDL control sequences |
| [open-control-library](https://github.com/jscott3201/open-control-library) | Justin Scott | MIT OR Apache-2.0 | `124427b` | Fault rules and test vectors for the benchmark; six rules (card, CXF and vectors) and four point dictionaries copied unmodified into `fixtures/open-control-library/` as test data |
| [rusty-bacnet](https://github.com/jscott3201/rusty-bacnet) | Justin Scott | MIT | `1caf4db` (v0.14.0) | BACnet/IP server, objects, types and transport for the read-only mirror; its client in tests |
| [BOPTEST](https://github.com/ibpsa/project1-boptest) | IBPSA Project 1 | BSD-3-Clause style | `9b1610b` (v0.9.0) | Large office building emulator and KPIs |
| [MinIO](https://github.com/minio/minio) and [mc](https://github.com/minio/mc) | MinIO, Inc. | AGPL-3.0 | `f19c534` and `1e78af4` (the releases BOPTEST v0.9.0 names) | BOPTEST's object store, built from source by `deploy/minio` and `deploy/modal` because those releases are no longer published as images; not redistributed |
| [shadcn/ui](https://github.com/shadcn-ui/ui) components | shadcn | MIT | CLI 4.21.4, style radix-nova | UI primitives copied into `web/src/components/ui` by the shadcn CLI, used unmodified except that they import our own `cn` helper |
| [React Flow](https://github.com/xyflow/xyflow) (`@xyflow/react`) | xyflow | MIT | 12.12.0 | Draws block graphs (pan, zoom, typed port handles); the layered layout is our own (`web/src/features/graph/layout.ts`) |
| [rmcp](https://github.com/modelcontextprotocol/rust-sdk) (official MCP Rust SDK) | Model Context Protocol project | Apache-2.0 | 3.5.1 (crates.io, pinned `=3.5.1`; features `server`, `transport-io`) | The MCP protocol side of `crates/mm-mcp` (JSON-RPC over stdio, version negotiation, handshake, discovery); the tools behind it are our own. Brings in [schemars](https://github.com/GREsau/schemars) 1.2.3 (MIT) and a few other ordinary crates (chrono, uuid, tokio-util), all in `Cargo.lock` |
| Node.js, Rust and Debian official container images | OpenJS Foundation, Rust project, Debian (Docker Official Images) | MIT, MIT OR Apache-2.0, DFSG licenses | `node:26.12.1-trixie-slim`, `rust:1.97.1-slim-trixie`, `debian:trixie-slim`, each pinned by digest in `Dockerfile` | Build and runtime base images for the packaged app (`Dockerfile`, [docs/packaging.md](docs/packaging.md)); unmodified |

Ordinary crates.io libraries (such as serde, reqwest, tokio and thiserror) and npm packages (such as React, Vite, Tailwind CSS and Radix UI) are used unmodified under their own licenses; `Cargo.lock` and `web/package-lock.json` pin every version. The Geist font (`@fontsource-variable/geist`) is under the SIL Open Font License 1.1.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
