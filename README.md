# Mechanical Muse

**The muse proposes. The engine proves.**

Mechanical Muse is an agent built on Meta's Muse Spark that writes building control logic from a written sequence of operations and proves it correct before it runs. A deterministic control engine checks every draft against test vectors, and only verified logic goes live on a simulated large office building.

Built solo by Justin Scott for the Meta Global AI Developer Hackathon (Indie track), October 26 to November 4, 2026.

> Status: work in progress during the hackathon.

## Built during the hackathon

Everything in this repository was written during the event, starting October 26, 2026, except that nine skills in `.agents/skills/` are trimmed forks of a pre-existing library (listed below). The commit history is the record.

| Component | Path | Status |
| --- | --- | --- |
| Engine bridge (JSON wrapper and CLI) | `crates/` | Planned |
| Block-graph schema and CXF emitter | `crates/` | Planned |
| Muse Spark authoring agent | `crates/` | Planned |
| Library benchmark harness | `crates/` | Planned |
| BOPTEST runtime host | `crates/` | Planned |
| BACnet device mirror | `crates/` | Planned |
| Web app | `web/` | Planned |
| Design notes and API references | `docs/` | Started |
| Coding-agent setup: agent context, project skills, verification hook | `AGENTS.md`, `.agents/skills/`, `.muse/`, `.mcp.json`, `scripts/` | Started |

The application's model calls all go to Meta's models (Muse Spark through the Meta Model API; Muse Glimmer for the open-weights comparison). The repository was built with Meta's Muse Code and with Claude Code as coding tools.

## Pre-existing dependencies (not built during the hackathon)

These are public, open-source projects pinned to commits from before the event and used unmodified.

| Dependency | Author | License | Pinned commit | Used for |
| --- | --- | --- | --- | --- |
| [open-control-engine](https://github.com/jscott3201/open-control-engine) | Justin Scott | MIT OR Apache-2.0 | `4063202` | Executing and verifying CDL control sequences |
| [open-control-library](https://github.com/jscott3201/open-control-library) | Justin Scott | MIT OR Apache-2.0 | `124427b` | Verified fault rules and test vectors for the benchmark |
| [rusty-bacnet](https://github.com/jscott3201/rusty-bacnet) | Justin Scott | MIT | `1caf4db` (v0.14.0) | Simulated BACnet devices |
| [BOPTEST](https://github.com/ibpsa/project1-boptest) | IBPSA Project 1 | BSD-3-Clause style | TBD | Large office building emulator and KPIs |
| [yet-more-skills](https://github.com/jscott3201/yet-more-skills) | Justin Scott | MIT OR Apache-2.0 | `97d2a36` | Source of nine trimmed skill forks in `.agents/skills/` |

`.agents/skills/diagnosis-loop` also carries an upstream MIT notice from [mattpocock/skills](https://github.com/mattpocock/skills); see its `NOTICE` file.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
