# Mechanical Muse

**The muse proposes. The engine proves.**

Mechanical Muse is an agent built on Meta's Muse Spark that writes building control logic from a written sequence of operations and proves it correct before it runs. A deterministic control engine checks every draft against test vectors, and only verified logic goes live on a simulated large office building.

Built solo by Justin Scott for the Meta Global AI Developer Hackathon (Indie track), October 26 to November 4, 2026.

> Status: work in progress during the hackathon.

## Run it

You need Rust (see `rust-toolchain.toml`), Node (`web/.nvmrc`) and a BOPTEST v0.9.0 checkout
(`git clone --branch v0.9.0 --depth 1 https://github.com/ibpsa/project1-boptest`).

1. BOPTEST, either locally with Docker (`export BOPTEST_DIR=<checkout>; make boptest-up`, slow on
   Apple silicon) or on Modal with native x86 (`make boptest-modal`, see
   [docs/boptest-on-modal.md](docs/boptest-on-modal.md)).
2. The server: `make server` (set `MM_BOPTEST_URL` for Modal). For live Muse Spark authoring, put
   a Meta Model API key in `.env` as `MODEL_API_KEY` and run `make server-live`.
3. The web app: `cd web && npm ci && npm run dev`, then open the printed address.

Without BOPTEST or a model key, the web app can still verify the specs, play back the recorded
Muse Spark session (labelled "recorded"), and replay recorded BOPTEST runs (labelled "replay").

## Built during the hackathon

Everything in this repository was written during the event, starting October 26, 2026. The commit history is the record.

| Component | Path | Status |
| --- | --- | --- |
| Engine bridge: load receipts, frame stepping, vector replay | `crates/mm-bridge` | Working offline: replays all 1,760 library scenarios on the pinned engine |
| Block-graph schema and CXF emitter | `crates/mm-graph` | Working offline: validator and emitter round-trip all 137 library rules |
| Muse Spark authoring agent: prompt, Responses client, draft-verify-repair loop | `crates/mm-agent` | Working live against Muse Spark (`docs/model-api.md` section 10); reports progress for live views; plays back recorded runs. With vector feedback, Spark drafts a passing chilled-water plant reset in 2 of 3 runs (`docs/benchmark-results.md`) |
| Library benchmark harness: scrubbed tasks, baselines, run records | `crates/mm-bench` | First results in `docs/benchmark-results.md` |
| BOPTEST runtime host: supervisory loop, BOPTEST client, fail-safe | `crates/mm-runtime` | Runs the verified AHU requests and CHW plant reset against BOPTEST live; 10-hour comparison with the baseline in `docs/runtime-results.md` |
| HTTP API: specs, typed block graphs, verification receipts | `crates/mm-server` | Working: spec, graph and verify endpoints; authoring sessions (Muse Spark, or a labelled recorded run) streamed as server-sent events, with verified drafts stored with their receipts; BOPTEST run sessions that can run a drafted sequence after replaying its vectors again, or inject a labelled test fault; labelled replays of recorded runs; run records; generated TypeScript types |
| BACnet device mirror | `crates/` | Planned |
| Web app | `web/` | Working: specs and verification receipts; Muse Spark authoring followed attempt by attempt (engine refusals, failing scenarios, the green receipt; recorded runs labelled); start BOPTEST runs on reference or drafted graphs and follow them live (status, receipts, trends, KPIs, event log); block graph view of reference and drafted graphs, with the blocks a problem names outlined; replays of recorded runs |
| Design notes and API references | `docs/` | Started |
| Coding-agent setup: agent context, project skills, verification hook | `AGENTS.md`, `.agents/skills/`, `.muse/`, `.mcp.json`, `scripts/` | Started |

The application's model calls all go to Meta's models (Muse Spark through the Meta Model API; Muse Glimmer for the open-weights comparison). The repository was built with Meta's Muse Code and with Claude Code as coding tools.

## Pre-existing dependencies (not built during the hackathon)

These are public, open-source projects used unmodified, each pinned to one commit. All but open-control-engine are pinned to commits from before the event. open-control-engine is developed independently in its own public repository; on October 27 we moved our pin to its `development` branch at `fa5cb19`, which includes engine fixes made during the event (some prompted by gaps we reported, listed in `docs/upstream-gaps.md`). Those engine changes are not part of this entry. On the new pin every verdict in our benchmark records is unchanged (`docs/benchmark-results.md`).

| Dependency | Author | License | Pinned commit | Used for |
| --- | --- | --- | --- | --- |
| [open-control-engine](https://github.com/jscott3201/open-control-engine) | Justin Scott | MIT OR Apache-2.0 | `fa5cb19` (`development`, 2026-10-27) | Executing and verifying CDL control sequences |
| [open-control-library](https://github.com/jscott3201/open-control-library) | Justin Scott | MIT OR Apache-2.0 | `124427b` | Fault rules and test vectors for the benchmark; six rules (card, CXF and vectors) and four point dictionaries copied unmodified into `fixtures/open-control-library/` as test data |
| [rusty-bacnet](https://github.com/jscott3201/rusty-bacnet) | Justin Scott | MIT | `1caf4db` (v0.14.0) | Simulated BACnet devices |
| [BOPTEST](https://github.com/ibpsa/project1-boptest) | IBPSA Project 1 | BSD-3-Clause style | `9b1610b` (v0.9.0) | Large office building emulator and KPIs |
| [MinIO](https://github.com/minio/minio) and [mc](https://github.com/minio/mc) | MinIO, Inc. | AGPL-3.0 | `f19c534` and `1e78af4` (the releases BOPTEST v0.9.0 names) | BOPTEST's object store, built from source by `deploy/minio` and `deploy/modal` because those releases are no longer published as images; not redistributed |
| [shadcn/ui](https://github.com/shadcn-ui/ui) components | shadcn | MIT | CLI 4.21.4, style radix-nova | UI primitives copied into `web/src/components/ui` by the shadcn CLI, used unmodified except that they import our own `cn` helper |
| [React Flow](https://github.com/xyflow/xyflow) (`@xyflow/react`) | xyflow | MIT | 12.12.0 | Draws block graphs (pan, zoom, typed port handles); the layered layout is our own (`web/src/features/graph/layout.ts`) |

Ordinary crates.io libraries (such as serde, reqwest, tokio and thiserror) and npm packages (such as React, Vite, Tailwind CSS and Radix UI) are used unmodified under their own licenses; `Cargo.lock` and `web/package-lock.json` pin every version. The Geist font (`@fontsource-variable/geist`) is under the SIL Open Font License 1.1.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
