---
name: rust-ts-contracts
description: "Guides Rust DTOs, generated TypeScript types, SSE event payloads and JSON edge cases across mm-bridge, mm-server and web/. Use when adding or changing an endpoint, event or DTO."
license: MIT OR Apache-2.0
metadata:
  short-description: "Rust DTOs, generated TS types, SSE payloads"
---
# Rust and TypeScript Contracts

The Rust DTO owns the wire contract. TypeScript types are generated from it; hand-written TS mirrors or edits to generated files are defects.

## Authority and order
- DTOs live in `mm-bridge` or `mm-server` with serde derives. New endpoint: DTOs, then the route, then regenerate the TS types, then the web hook and view.
- The generator (ts-rs, specta or schemars) is a pending decision in `AGENTS.md`: use what the workspace already runs and do not add a second. Keep adapters outside the generated directory.

## Wire rules
- `Value` has no `PartialEq` or `Serialize`: go through a `WireValue` DTO (`Real`, `Integer`, `Boolean`, `Enum { class, ordinal }`). Enum ordinals are 1-based.
- JSON cannot carry NaN or infinities. Encode non-finite reals as the strings `"NaN"`, `"Infinity"` and `"-Infinity"`, type them in TS as `number | "NaN" | "Infinity" | "-Infinity"`, and decode in one helper. Never `unwrap` serialization of frame data.
- Engine `Integer` fits i32, safe as a JS number. Any u64 or i64 that can pass 2^53-1 (counters, ids) needs an agreed lossless form such as a string; decide once per field and test `9007199254740993`.
- A point's identity is its CXF `@id` everywhere; derive display names in one place and invent no parallel names.
- Diagnostics are `{stage, code, severity, subject, message}`; the UI switches on `code` and only displays `message`. Map `#[non_exhaustive]` error enums with an `"other"` arm.
- SSE payloads are one tagged union (`#[serde(tag = "kind")]`) behind the envelope `{run_id, seq, sim_time_s, kind, ...}`; `sim_time_s` is BOPTEST simulation time. In TS, narrow on `kind`, end the `switch` with a `never` check, and ignore unknown kinds without crashing.
- Missing, null, zero and not-evaluated differ. Never turn absent telemetry into 0; pick `skip_serializing_if` or explicit null per field and test the serialized JSON.
- DTOs and error bodies never carry the API key or echo raw prompts or payloads.

## Prove the boundary
- Shared JSON fixtures under `fixtures/` are read by both `cargo test` and the web unit tests: accepted and rejected cases, non-finite strings, an unknown `kind`, absent versus null, an unknown enum value. Do not regenerate expectations to make a fixture pass.
- Type-checking and runtime parsing answer different questions; run `cargo test --workspace` and the web typecheck and tests.
- Cases to cover: [contract cases](references/cases.md)

<!-- Adapted from yet-more-skills/cross-language-contracts and yet-more-skills/typescript-contracts (MIT OR Apache-2.0) -->
