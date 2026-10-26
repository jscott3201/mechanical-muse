---
name: mm-block-graph-cxf
description: "Guides the block-graph JSON schema, catalog palette, validator and deterministic CXF emitter in mm-graph. Use when changing graph types, the model-facing schema, emitter output or goldens."
license: MIT OR Apache-2.0
metadata:
  short-description: "Block-graph schema, palette, validator, CXF emitter"
---
Flow: model JSON (`docs/block-graph.schema.json`) -> validator -> deterministic emitter -> CXF bytes -> `mm-bridge`. Never hand-write CXF; if the emitter cannot express something, extend `mm-graph` and add a golden. Depth: `docs/block-graph-schema.md`.

**Format**
- `{name, inputs[{name,type,unit,description}], outputs[...], blocks[{id,class,params[{name,value}]}], connections["a.y -> b.u"]}`. Class is a catalog path minus `CDL.` (`Logical.TrueDelay`). A bare token is a boundary name, `id.port` a block port; every block input and every output has exactly one driver.
- The task locks the interface: names and types must match exactly. Names are at least 2 chars, dot-free, unique across inputs, outputs and block ids, never a port name (`u`, `y`, `u1`).

**Schema stays inside Meta's strict subset** (`mm-meta-model-api`)
- Closed objects, all properties required; no `oneOf`/`allOf`, recursion, `pattern`, `minItems`; depth <= 10; <= 1,000 enum values. So params are an array and edges are strings.
- `params[].value` is `anyOf[number,boolean,string]`: live decoder acceptance is unverified, spike it (fallbacks: `docs/block-graph-schema.md` section 8).
- Keep `Reals.PID` and `Reals.PIDWithReset` out of the class enum until the emitter has a golden for enum params (`controllerType`).

**Palette** from `oce_api::catalog()`, never typed by hand
- 136 entries; skip `reserved` (3 PassThrough) and `width_driven` (25) until port synthesis exists. Ports, `param_defaults` and `param_rules` feed the enum and prompt.
- `CATALOG_JSON` stores Real defaults and bounds as 16-hex-digit binary64 bits (`{"kind":"real","bits":"3ff0..."}`): decode with `f64::from_bits`.
- State port order in prompts (`Reals.Subtract` is u1 - u2). Default `delayOnInit=true` on `Logical.TrueDelay` in prompt or emitter (catalog default false).
- No Trim and Respond or other composite: compose elementary blocks (`docs/plant-reset.md` section 1.4). Upstream has an oracle only for `have_hol=false`; ours is `true`, so our vectors are the oracle.

**Validator** (before the engine)
- Class in palette; params known, `Required` present; endpoints exist; port kinds match; nothing undriven or doubly driven. Mutation tests (dropped edge, second driver, unknown class, missing param, swapped boundary type) must each be flagged.

**Emitter** (determinism, AGENTS rule 6)
- Bytes depend only on the graph: explicit order, no `HashMap` iteration, no clock. Rules: `docs/block-graph-schema.md` section 5 (inline `@context`, `S231:isConnectedTo` on the source, Reals printed with a decimal point such as `86400.0`); `unit` and `description` are dropped.
- Goldens in `crates/mm-graph/tests/golden/`; refresh only with `UPDATE_GOLDEN=1` and review the diff. Also load each golden with a receipt: zero records, stable content id.
