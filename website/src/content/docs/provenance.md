---
title: Provenance
description: What was built during the hackathon, and the pre-existing open-source dependencies Mechanical Muse uses unmodified.
---

This page mirrors the README. If the two ever disagree, the
[README](https://github.com/jscott3201/mechanical-muse#pre-existing-dependencies-not-built-during-the-hackathon)
is the record.

## Built during the hackathon

Everything in this repository was written during the event, starting October 26, 2026, except
the pre-existing material listed below (for example the open-control-library rules copied
unmodified into `fixtures/open-control-library/` as test data). The commit history is the record.

Built in the event: the engine bridge (`mm-bridge`), the block-graph schema and CXF emitter
(`mm-graph`), the Muse Spark authoring agent (`mm-agent`), the control sequence specs
(`specs/`), the benchmark harness (`mm-bench`), the BOPTEST runtime host (`mm-runtime`), the
HTTP API (`mm-server`), the MCP server (`mm-mcp`), the BACnet device mirror (`mm-bacnet`), the
web app (`web/`), the design notes (`docs/`), this documentation site (`website/`), and the
coding-agent setup. The README's
["Built during the hackathon"](https://github.com/jscott3201/mechanical-muse#built-during-the-hackathon)
table gives each one's status.

Every model call the application makes goes to Muse Spark through the Meta Model API: drafting,
repair, reading a sequence, proposing a point binding, explanations and the benchmark.

## Pre-existing dependencies (not built during the hackathon)

These are public, open-source projects used unmodified, each pinned to one commit. All but
open-control-engine are pinned to commits from before the event. open-control-engine is
developed independently in its own public repository; on October 27 we moved our pin to its
`development` branch at `fa5cb19`, which includes engine fixes made during the event (some
prompted by gaps we reported, listed in `docs/upstream-gaps.md`). Those engine changes are not
part of this entry. On the new pin every verdict in our benchmark records is unchanged
(`docs/benchmark-results.md`).

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
| [Astro](https://github.com/withastro/astro) and [Starlight](https://github.com/withastro/starlight) (`astro`, `@astrojs/starlight`) | The Astro project (withastro) | MIT | 7.3.8 and 0.42.6 (npm, exact; `website/package-lock.json`) | Builds this documentation site from our own pages (`website/`); used unmodified. Brings in Pagefind (search, MIT), Expressive Code (code blocks, MIT) and sharp (Apache-2.0, with prebuilt libvips under LGPL-3.0-or-later), which resizes images at build time and is not shipped in the site |
| [Modal](https://modal.com) (`modal` Python SDK and CLI) | Modal Labs | Apache-2.0 (client); hosted service | Not pinned (the developer's installed CLI) | A test host: BOPTEST on native x86 for our own live runs (`deploy/modal/mm_boptest.py`, [docs/boptest-on-modal.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/boptest-on-modal.md)) and an x86 build check of the app image (`deploy/modal/mm_demo.py`); not a hosted demo, and nothing in the app depends on it |
| Node.js, Rust and Debian official container images | OpenJS Foundation, Rust project, Debian (Docker Official Images) | MIT, MIT OR Apache-2.0, DFSG licenses | `node:26.12.1-trixie-slim`, `rust:1.97.1-slim-trixie`, `debian:trixie-slim`, each pinned by digest in `Dockerfile` | Build and runtime base images for the packaged app (`Dockerfile`, [docs/packaging.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/packaging.md)); unmodified |

Ordinary crates.io libraries (such as serde, reqwest, tokio and thiserror) and npm packages (such
as React, Vite, Tailwind CSS and Radix UI) are used unmodified under their own licenses;
`Cargo.lock`, `web/package-lock.json` and `website/package-lock.json` pin every version. The
Geist font (`@fontsource-variable/geist`) is under the SIL Open Font License 1.1.

The control sequence specs in `specs/` are written in our own words. The
[Modelica Buildings Library](https://github.com/lbl-srg/modelica-buildings) (BSD-3-Clause) served
as a behavioral reference for how such sequences act; nothing from it is copied into this
repository, and no endorsement by its authors is implied. Nothing here reproduces text, tables or
figures from any published standard.
