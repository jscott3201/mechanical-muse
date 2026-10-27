# Decisions

Decisions for Mechanical Muse. Muse Code's `/grill-with-docs` writes here. Newest first within each list.

## Pending (do not guess: ask, or pick the default and note it here)

- TODO(decision): clippy strictness (pedantic subset) and the allowed-warning list for loads.
- TODO(decision): BACnet device instance, UDP port, and the placeholder vendor id constant.
- TODO(decision): Glimmer serving stack on Modal (vLLM | SGLang) and its OpenAI-compatible URL.

## Decided

- BOPTEST host: local Docker on Apple silicon (v0.9.0 via `compose.yaml`; the x86 worker runs
  under emulation), 120 s control step, and demo episodes of hours, not BOPTEST's 14-day
  scenarios. Measured on the dev machine (2026-10-26), `multizone_office_complex_air`:
  select 2.5 min on a free worker (17 min when queued behind an orphaned test, so always stop
  tests); initialize 7 min with a 1 h warm-up and 13 to 19 min with a 1-day warm-up (mostly
  fixed cost); 60 steps of 120 s at a mean 9.4 s each (median 9.6, max 17.0), 12.7x faster than real
  time in that January window; a summer morning ran at 0.84 s per step (`docs/runtime-results.md`),
  so step cost depends on the period. At the slow rate one simulated day is about 2 h of wall
  time and a 14-day scenario about 26 h, so the demo uses a few simulated hours, recorded ahead,
  on a weekday near the peak cooling day (day 185, a Monday) with a 1-day warm-up. An x86 VM stays the fallback if a longer episode is needed.
  (2026-10-26)

- CI: GitHub Actions adapted from rusty-bacnet (`docs/ci.md`): pinned toolchain, SHA-checked
  tools, SHA-pinned actions, nextest as the test runner, cargo-deny, and one aggregate `CI OK`
  status. (2026-10-26)

- Task runner: `make` (preinstalled everywhere, including Muse's sandbox). `make check` runs the
  same fmt, clippy and test commands as the Stop hook. (2026-10-26)
- Web package manager: npm with Node 26, pinned in `web/.nvmrc`; the Stop hook already calls npm.
  (2026-10-26)
- Rust-to-TS types: ts-rs. DTOs derive `TS` and a test run exports the `.ts` files; no hand-written
  mirrors. (2026-10-26)

- Coding harnesses: Muse Code and Claude Code, used interchangeably to build. The organizers
  confirmed in the event's rules channel (2026-10-26) that any build tools are allowed, Claude Code
  included, and that non-Meta models may be used alongside Meta's as long as Meta's models are a
  real part of what the project runs. Our line is stricter: every model call the application makes
  goes to Meta's models (AGENTS.md hard rule 8). One context file (`AGENTS.md`), committed skills in
  `.agents/skills/`, one Stop hook script (`.muse/hooks/verify.sh`), MCP servers in `.mcp.json`.
  `.claude/` (Claude Code settings and its skills copy) and `ledger/` (shared handoff log) stay
  local and gitignored. See `docs/coding-agents.md`.
- Hero sequence: chilled-water plant reset written from the Modelica Buildings Library PlantReset
  documentation (paraphrased). Hot-water reset is a stretch.
- Building emulator: BOPTEST `multizone_office_complex_air`; EnergyPlus 26.2 Large Office is the fallback.
- Meta endpoint: `POST /v1/responses`. Model `muse-spark-1.3` for the agent and demo;
  `muse-spark-1.3-contributor` only for benchmark runs on public library data (it opts into
  training-data use, so never send it non-public data). Key env var: `MODEL_API_KEY` (Muse Code
  itself reads `META_API_KEY`).
- Event transport to the browser: server-sent events.
- Licenses: MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE` at the root).
- First live loop is supervisory; actuator-level VAV control on one floor is a stretch.
- Muse Glimmer served on Modal is planned (benchmark comparison and one demo run; targets the
  open-weights bonus). Keep the model behind the `ModelClient` trait so Spark, Wonder, and the
  Glimmer endpoint are interchangeable.
- Judging rubric: Impact 30%, Use of Meta AI tooling 25%, Technical depth 20%, Demo 25%. The
  demo video is three minutes and the live product must hold up for the final round.
