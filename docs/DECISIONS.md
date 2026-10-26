# Decisions

Decisions for Mechanical Muse. Muse Code's `/grill-with-docs` writes here. Newest first within each list.

## Pending (do not guess: ask, or pick the default and note it here)

- TODO(decision): task runner (`just` | `cargo xtask` | `make`).
- TODO(decision): web package manager (pnpm | npm) and Node version.
- TODO(decision): Rust-to-TS type generation tool.
- TODO(decision): BOPTEST host (local Rosetta vs x86 VM), compose layout, step size, and demo
  episode length. Decided after the O1 speed benchmark.
- TODO(decision): clippy strictness (pedantic subset) and the allowed-warning list for loads.
- TODO(decision): BACnet device instance, UDP port, and the placeholder vendor id constant.
- TODO(decision): Glimmer serving stack on Modal (vLLM | SGLang) and its OpenAI-compatible URL.

## Decided

- Coding harness: Meta's Muse Code CLI only. One context file (`AGENTS.md`), project skills in
  `.agents/skills/`, a Stop hook in `.muse/hooks.json`, MCP servers in `.mcp.json`. See `docs/muse-code.md`.
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
