# oce-api and rusty-bacnet: API reference for Mechanical Muse

Written during the hackathon (Oct 26, 2026) as a reference for coding agents working in this repo.
Every signature below was read from the pinned sources or the crate's own
`crates/oce-api/tests/public-api.txt` baseline, and the two code snippets in sections 12 and 17
were compiled and run against the pinned revs (edition 2024, Rust 1.97.x).

Pinned sources (read-only for the whole event):

| Repo | Tag | Rev (use this in Cargo.toml) |
|---|---|---|
| open-control-engine | v0.2.0 (main) | `4063202b9a9e50c723bb858d786108571d3d401d` |
| rusty-bacnet | v0.14.0 | `1caf4db71ac488720b17e343b9c50d43c15f62fc` |
| open-control-library | (data only) | no code dependency; use as test-vector and CXF reference |

The engine rev above (tag v0.2.0) is tree-identical to development commit `4196a8d...` that some
engine docs mention; either spelling is the same source.

## 1. Mental model

- `oce-api` is the single public facade of the engine. Depend on it and nothing else from that
  workspace. It re-exports `Value`, `ValueType`, `ConnectorId`, `Diagnostic`, and the `oce_store`
  crate (seam DTOs).
- The engine executes a control sequence. It does not supervise equipment, judge data quality, own
  time, persist anything, or implement fail-safe behavior. All of that is the host's job (see
  `docs/host-responsibilities.md` in the engine repo). In this project the host is `mm-runtime`.
- The only state-advancing surface is frame-based: `prepare_frame` then `execute_frame`. The old
  `tick` / `set_input` / `simulate` calls were removed.
- `Engine<S>` is `Send + Sync`. Execution needs `&mut Engine`, so one owner task per engine.
- `Engine::in_memory()` is the only constructor you need: `Engine<MemStore>`. `with_store(Arc<S>)`
  exists for custom stores; do not use it.

## 2. Depending on it

```toml
# Cargo.toml (workspace member crates inherit via [workspace.dependencies])
[workspace.dependencies]
oce-api = { git = "https://github.com/jscott3201/open-control-engine", rev = "4063202b9a9e50c723bb858d786108571d3d401d" }
bacnet-objects = { git = "https://github.com/jscott3201/rusty-bacnet", rev = "1caf4db71ac488720b17e343b9c50d43c15f62fc" }
bacnet-server  = { git = "https://github.com/jscott3201/rusty-bacnet", rev = "1caf4db71ac488720b17e343b9c50d43c15f62fc" }
bacnet-types   = { git = "https://github.com/jscott3201/rusty-bacnet", rev = "1caf4db71ac488720b17e343b9c50d43c15f62fc" }
```

- Cargo resolves workspace-member crates from a git repo by crate name, so no `path` or `package`
  key is needed. `oce-api` is at `crates/oce-api` inside the engine repo.
- Toolchain: engine is edition 2024, resolver 3, MSRV 1.97.0, its `rust-toolchain.toml` pins
  1.97.1. Match it in `rust-toolchain.toml` and set `rust-version = "1.97.0"` so our crates never
  need a newer compiler than our dependency. rusty-bacnet's MSRV is lower (1.93 at v0.14.0), so
  the engine sets the floor.
- Cargo features: leave the defaults. `default = ["mem"]`, and `mem` is a legacy no-op spelling
  (`oce-store-mem` is an unconditional dependency). There is no database or async runtime in the
  default build; the engine is synchronous.
- `cargo update` must never move these. Commit `Cargo.lock` (this is a binary workspace).
- Do not depend on the library repo's verifier (`tools/verify`, `ENGINE_PIN` `41e997f...`). It
  targets the removed tick API and will not compile against v0.2.0. See section 13.

## 3. Public surface (grouped, exact signatures)

Names are all in the crate root `oce_api::`. `S = oce_store::Store`, `OcResult<T> = Result<T, OcError>`.

### Construction and loading

```rust
Engine::<MemStore>::in_memory() -> Self
Engine::with_store(Arc<S>) -> Self
engine.load_cxf(&mut self, &[u8]) -> Result<LoadReport, OcError>
engine.load_cxf_with_receipt(&mut self, &[u8]) -> Result<LoadReceipt, OperationFailure>   // prefer this
engine.cxf_byte_limit(&self) -> usize            // default MAX_CXF_BYTES = 8 MiB
engine.set_cxf_byte_limit(&mut self, usize) -> OcResult<()>
engine.export_cxf(&self) -> OcResult<ExportReport>                       // bytes + warnings
engine.export_cxf_with_receipt(&self) -> Result<ExportReceipt, OperationFailure>
```

- `LoadReport { block_count: usize, stateful_blocks: usize, io: IoSummary, model_id: DomainKey,
  warnings: Vec<Diagnostic> }` (`#[non_exhaustive]`, `Default`).
- `LoadReceipt::{ report() -> &LoadReport, diagnostics() -> &DiagnosticReceipt,
  into_parts() -> (LoadReport, DiagnosticReceipt) }`.
- `ExportReport { bytes: Vec<u8>, warnings: Vec<Diagnostic> }`, plus `content_id() -> String` and
  `content_id_complete() -> Result<String, ContentIdError>` (errors if warnings exist).
- Loading replaces the model wholesale. A second `load_cxf` on the same engine is allowed but see
  `docs/host-responsibilities.md` ("Load replacement and the Store compensation boundary"). For us:
  build a fresh `Engine` per candidate model. It is cheap.

### Frames (the HostTick transition)

```rust
engine.input_definitions(&self) -> OcResult<Vec<InputDefinition>>
engine.prepare_frame(&self, time: f64, entries: &[(&str, Value)]) -> OcResult<PreparedInputFrame>
engine.execute_frame(&mut self, PreparedInputFrame) -> OcResult<CompletedFrame>
```

- `InputDefinition { path: String, value_type: ValueType, min: Option<Value>, max: Option<Value> }`.
  Returned in canonical (lexical UTF-8) path order. `path` is the CXF `@id` of the boundary input.
- `PreparedInputFrame` is opaque and consumed by `execute_frame` (cannot be reused; it carries a
  load/rebuild generation, so a frame prepared before a reload or a halt/set/resume cycle returns
  `StalePreparedFrame`).
- `CompletedFrame`:
  - `time() -> f64`
  - `sequence() -> u64` (1-based per Engine lifetime; never reset by reload/resume/restore)
  - `inputs() -> &[(String, Value)]` (canonical order)
  - `outputs() -> &[(String, Value)]` (all executable boundary outputs, lexical order)
  - `diagnostics() -> &[AssertEvent]` (warnings only; `AssertEvent { block: String, message:
    String, t: f64, level: AssertLevel }`, `AssertLevel::Warning` is the only variant)
  - `replay_record() -> Result<ReplayRecord, ReplayError>`
- `prepare_frame` keys are the canonical paths from `input_definitions`. Treat those as the only
  contract; the engine's internal resolver may accept other spellings, which are not promised.

### Observation (latest state, not a receipt)

```rust
engine.get_output(&self, &str) -> OcResult<Value>                       // connector path or boundary-output alias
engine.watch(&self, &[&str]) -> OcResult<Vec<(String, Value)>>          // echoes keys incl. duplicates; empty slice is Ok(empty)
engine.io(&self) -> &IoInventory                                        // iter()/get(i)/len()/to_vec()
engine.io_summary(&self) -> IoSummary
engine.point_list(&self, controlled_device: Option<&str>) -> OcResult<Vec<PointInfo>>
engine.topology(&self) -> Topology
engine.schedule(&self) -> &oce_graph::Schedule                          // opaque to us
```

- Use `CompletedFrame::outputs()` for anything correlated with a step. `get_output` / `watch` read
  latest state, which load, restore, and resume can replace without running a frame.
- `PointInfo { path, direction: PointDirection, io_class: IoClass, physical: PhysicalKind,
  value_type: PointValueType, unit, quantity, display_unit, min, max (Option<f64>), hardwired,
  in_pointlist, trend: Option<TrendCfg> }`.
  `IoClass::{AnalogInput, AnalogOutput, DigitalInput, DigitalOutput, Network}`,
  `PhysicalKind::{Sensor, Actuator, SoftwarePoint}`. This is what drives the BACnet object map
  (AnalogInput/AnalogOutput/BinaryInput/BinaryOutput) in `mm-bacnet`.
- `IoSummary { analog_inputs, analog_outputs, digital_inputs, digital_outputs, network, total }`
  (all `usize`).
- `Topology { blocks: Vec<TopologyBlock>, connections: Vec<TopologyConnection>, external_inputs:
  Vec<String>, boundary_outputs: Vec<DeclaredOutput>, pass_through: Vec<PassThroughPair> }`.
  `TopologyBlock { instance_path, class_iri, inputs, outputs, params: Vec<(String, Value)> }`,
  `TopologyConnection { from, to }`. This is the data source for the web app's graph view.

### Parameters and lifecycle

```rust
engine.params(&self) -> &ParamTable          // iter()/to_vec(): (path, Value, ParamAttrs)
engine.get_param(&self, &str) -> OcResult<Value>
engine.mode(&self) -> RunMode                // Running | Halted
engine.halt(&mut self) -> OcResult<()>
engine.set_param(&mut self, &str, Value) -> OcResult<()>   // only when Halted
engine.resume(&mut self) -> OcResult<()>
```

- `ParamAttrs { value_type, unit: Option<String>, quantity: Option<String>, min/max: Option<f64> }`.
- `set_param` refuses unless `Halted` (`ParamWhileRunning`), checks type (`ParamType`), enum range,
  structural params (`ParamStructural`), declared bounds and class rules (`ParamRange`).
- `resume()` re-instantiates affected blocks and **resets the run clock**: a halt/set/resume cycle
  restarts the run (stateful blocks re-seed). Do not use it as a "tweak a gain live" mechanism in
  the runtime loop. The supervisory loop should pass resets as boundary inputs, not as params.
- While edits are pending (between `set_param` and `resume`), `input_definitions`, `prepare_frame`,
  `execute_frame`, `checkpoint` fail with `State(PendingParameterEdits)`.

### Checkpoint and state

```rust
engine.checkpoint(&self) -> OcResult<EngineCheckpoint>               // opaque, process-local, Clone
engine.restore_checkpoint(&mut self, &EngineCheckpoint) -> OcResult<()>   // may rewind a running engine
engine.state_snapshot(&self) -> OcResult<EngineStateSnapshot>        // canonical durable bytes
engine.restore_state(&mut self, &EngineStateSnapshot) -> OcResult<()>    // fresh engine only
EngineStateSnapshot::{as_bytes, into_bytes, from_bytes, portability}
```

- `restore_state` requires an engine that has accepted no frame, no dirty resume, and no restore
  since load. `restore_checkpoint` has no such limit. For the verify-then-promote flow use
  `checkpoint()` on the authoring engine only if you need rewind; otherwise just build a new engine.
- Snapshots can be target-bound (`StatePortability::TargetBound { os, arch }`) when a block needs
  it; never ship them between architectures.

### Replay records (optional, useful for the demo's evidence panel)

`CompletedFrame::replay_record()` captures inputs, outputs, warnings, and exact time bits.
`ReplayRecord::{as_bytes, from_bytes, content_id, check_compatible(&CompatibilityDescriptor),
verify(&CompletedFrame), inputs, outputs, diagnostics, time, exactness (ExactBits)}`. Cap
`MAX_REPLAY_BYTES` = 64 MiB (capture can refuse after a successful frame; that does not undo the
frame). Its content tag is a non-authoritative identifier, not a signature: authenticate bytes
outside the engine before trusting them. Verification re-executes nothing by itself.

### Catalog

```rust
oce_api::catalog() -> &'static [CatalogEntry]        // 136 entries, allocates once
oce_api::CATALOG_JSON: &str                           // same data, contracts/catalog.json
oce_api::catalog_to_json(&[CatalogEntry]) -> String
oce_api::catalog_content_id(&[CatalogEntry]) -> String   // "catalog:1:fnv1a128:<32 hex>"
oce_api::CATALOG_SCHEMA_REVISION: u32
```

- `CatalogEntry { class_path: &'static str, inputs: Vec<CatalogPort>, outputs: Vec<CatalogPort>,
  naming: CatalogPortNaming (Named|Positional|WidthDriven), param_defaults:
  Vec<CatalogParamDefault>, param_rules: Vec<CatalogRule>, reserved: bool, stateful: bool,
  width_driven: bool }`.
- `CatalogPort { name: Option<&'static str>, kind: CatalogPortKind (Real|Integer|Boolean) }`.
- `CatalogParamDefault { name, default: CatalogDefault }` where `CatalogDefault::{Required,
  Real(f64), Integer(i64), Boolean(bool), String(&str), EnumMember(&str), Derived(&str)}`.
- `CatalogRule` is a large enum of parameter constraints (e.g. `Required { name, kind }`,
  `RealFiniteGreaterThan`, `RealLessOrEqual { lower, upper }`, `IntegerArrayElements`, ...). It is
  the machine-readable source for "what parameters may this block take". Feed it into the LLM
  prompt and the graph schema generator.
- `class_path` is `CDL.<Package>.<Class>` (for example `CDL.Reals.Limiter`, `CDL.Logical.TrueDelay`,
  `CDL.Reals.Hysteresis` is stateful). One reserved entry, `urn:oce:lowering#PassThrough`, is an
  internal lowering class: never emit it.
- Only the 136 elementary CDL classes exist. There are no G36 composite blocks, no Trim and
  Respond, no PID-with-reset composite. Compose them from elementary blocks. See
  the block census (kept locally, not committed) for which classes the hero sequences need.

## 4. Value types

```rust
#[derive(Clone, Debug)]            // NO PartialEq, NO Serialize
pub enum Value { Real(f64), Integer(i64), Boolean(bool), String(Arc<str>),
                 Enum { class: EnumClassId, ordinal: u32 } }
#[derive(Clone, Copy, Debug, ...)] pub enum ValueType { Real, Integer, Boolean, String, Enum(EnumClassId) }
```

- Compare with `Value::bit_eq(&self, &Value) -> bool` (NaN equals itself, `0.0 != -0.0`, no
  cross-type coercion). Never use `==`; it does not compile.
- Accessors: `value_type()`, `as_real() / as_integer() / as_boolean() -> Result<_, TypeError>`.
- `String` is metadata only. It is never a boundary input (`prepare_frame` rejects it).
- `Enum { class, ordinal }`: ordinal is 1-based. `EnumClassId(pub u32)` is **not** re-exported from
  `oce_api`. You can still copy the value out of `InputDefinition::value_type` and construct
  `Value::Enum { class, ordinal }` without naming the type (the snippet in section 12 does this).
  Known ids: SimpleController=1, Smoothness=2, Extrapolation=3, ZeroTime=4.
- `Integer` carries `i64` but the logical domain is i32 (+-2,147,483,647).
- Unbounded `Real` inputs accept NaN and infinity. A declared `min`/`max` rejects NaN. The host must
  sanitize sensor values (reject or substitute NaN and infinities) before building a frame.

## 5. HostTick contract (what the host guarantees every frame)

From `docs/execution-profile.md` and `docs/host-responsibilities.md`:

1. Every call to `execute_frame` is one atomic state transition, even at an equal timestamp. Never
   resubmit a frame to "retry delivery": it advances the state and the sequence again.
2. `time` is finite, in seconds, nondecreasing (`TimeRegression { now, prev }`, `NonFiniteTime`).
   The engine does not read a clock. Use BOPTEST simulation time (seconds since the test case start)
   as `time`, not wall clock.
3. Every boundary input is required, exactly once, with the exact type. No sparse frames, no
   hold-last, no defaulting. A sequence with N inputs needs N values each step.
4. Any refusal (every `Err` from `prepare_frame` / `execute_frame`) happens before mutation. The
   engine state, sequence, and retained outputs are unchanged.
5. Warnings in `CompletedFrame::diagnostics()` never refuse execution. They are not alarms.
6. The engine does no fail-safe. If the sensor feed is stale or the engine refuses a frame, the
   host decides what the plant sees: hold last good setpoints, revert to the baseline controller,
   or stop. In our runtime: on refusal, do not write new setpoints; mark the sequence "degraded" in
   the UI.
7. `halt()` does not stop the tick loop. It only opens the tune-at-rest window.
8. Persistence, authentication, deployment fencing, scheduling, actuator interlocks, and the
   mapping of engine outputs to equipment are host-owned. The accepted-frame sequence is correlation
   only within one Engine lifetime.

## 6. Errors and diagnostics

### OcError (`#[non_exhaustive]`: always add a wildcard arm)

Frame-path: `FrameMissingInput(path)`, `FrameDuplicateInput(path)`, `FrameUnknownInput { prefix, bytes }`,
`FrameNotInput { prefix, bytes }`, `InputType(path)`, `InputDomain(path)`, `NonFiniteTime { now }`,
`TimeRegression { now, prev }`, `ModelTimeUnrepresentable { now }`, `StalePreparedFrame`,
`FrameSequenceExhausted`.
Load-path: `Cxf(CxfError)`, `Flatten(..)`, `Validate(..)`, `Build(..)`, `Load { detail }`,
`LoadContext(LoadErrorContext)`, `CxfTooLarge { actual_bytes, limit_bytes }`,
`CxfByteLimitTooLarge { .. }`, `Store(..)`.
Params/state: `ParamWhileRunning`, `ParamType`, `ParamRange`, `ParamStructural` (each `{ path }`),
`UnknownPoint(path)`, `State(EngineStateError)`.

`prepare_frame` reports only the first cause, in this fixed precedence: unloaded or pending edits,
nonfinite/regressing/unrepresentable time, unknown or non-boundary keys (lexical order), duplicate
input, missing input, then type/domain errors in canonical input order. Unknown-key errors truncate
the key to 64 bytes. That determinism is useful: the agent can be told exactly one thing to fix.

### Receipts (use `load_cxf_with_receipt`)

```rust
OperationFailure::{ stage() -> DiagnosticStage, diagnostics() -> &DiagnosticReceipt,
                    error() -> &OcError, into_error() -> OcError }   // impl Display + Error
LoadReceipt::diagnostics() -> &DiagnosticReceipt
DiagnosticReceipt::records() -> &[DiagnosticRecord]                  // sorted by key, no truncation
DiagnosticRecord::{ key() -> &DiagnosticKey, message() -> &str }     // message is NOT stable
DiagnosticKey::{ stage() , subject() -> &DiagnosticSubject, code() -> &str, severity() }
DiagnosticSubject::{ Absent, Opaque(String) }        // Opaque text is the authored @id or a class name
DiagnosticSeverity::{ Error, Warning, Info }
DiagnosticStage::{ Import, Flatten, AttributeUnification, Validation, Instantiation, Schedule,
                   Semantics, Projection, StoreRecovery, StoreSave, StoreInputs, Export }
```

- Machine order: stage rank, subject (Absent first, then exact text), code string, severity rank.
  Compare keys, never messages. Messages are for humans (and the LLM) but are not a compatibility
  promise.
- A failure receipt can be empty (JSON/build errors have no diagnostic code). Fall back to
  `stage()` plus `error()`.
- Stable code strings (`DiagCode::as_str`): duplicate-id, missing-connector-id, unresolved-reference,
  class-not-found, overlay-target-not-found, array-flatten-collision, grounding-failed,
  unknown-enum-type, unknown-enum-literal, enum-integer-standin, conditional-guard-unknown-parameter,
  conditional-guard-unsupported, inactive-conditional-node, missing-active-conditional-node,
  unresolved-polymorphism, non-subset-construct, malformed-document, conflicting-interface-declaration,
  boundary-output-shadows-connector, relative-iri, single-assignment, direction-mismatch, type-mismatch,
  port-name-mismatch, port-kind-mismatch, unit-quantity-mismatch, bound-mismatch,
  missing-required-parameter, parameter-kind-mismatch, duplicate-parameter, parameter-out-of-range,
  display-unit-divergence, analog-coerced-to-real, missing-fmu-path, unknown-property,
  undriven-boundary-output, export-unsupported, export-deferred.

### Observed diagnostics (real runs against the pinned engine)

| Mistake | Output |
|---|---|
| Unknown class `CDL.Logical.TrueDelayX` | stage `Import`, code `class-not-found`, Error: "no registered block class for `CDL.Logical.TrueDelayX`" (1 diagnostic) |
| Connection target id that does not exist | stage `Import`, two records: `unresolved-reference` ("connection target not found") and `single-assignment` ("input is undriven (in-degree 0)") |
| Frame missing an input column | `prepare_frame` -> `FrameMissingInput(<canonical path>)` (our CSV mapper caught it first) |
| Time going backwards (60 then 0) | row 0 accepted (seq 1), then `TimeRegression { now: 0.0, prev: 60.0 }` |

Both import failures are `Error` severity, so `load_cxf_with_receipt` returns `Err(OperationFailure)`.
The top-level Display text is "CXF ingest error: CXF validation failed with N diagnostic(s)"; always
iterate `diagnostics().records()` for the detail.

## 7. Serde gaps (our DTO layer is new work)

`oce-api` and `oce-model` have no `serde` dependency, so almost nothing is `Serialize`. The only
serde types reachable are the `oce_store` DTOs re-exported as `oce_api::oce_store::*` (for example
`DomainKey`, `PointDirection`, `PointValueType`, `TrendInterval`).

| Type | Serialize? | What `mm-bridge` should do |
|---|---|---|
| `Value` | no, no `PartialEq` | own `enum WireValue { Real(f64), Integer(i64), Boolean(bool), Enum{class:u32, ordinal:u32} }` with `From<&Value>`; non-finite reals as strings `"NaN"`/`"Infinity"` (JSON cannot carry them) |
| `InputDefinition`, `PointInfo`, `ParamAttrs`, `IoSummary` | no | mirror structs (`#[derive(Serialize)]`) and `From<&T>` |
| `CompletedFrame`, `AssertEvent` | no | `FrameDto { sequence, time, inputs, outputs, warnings }` |
| `DiagnosticRecord` / `DiagnosticKey` | no | `{ stage: String, code: String, severity: String, subject: Option<String>, message: String }` using `format!("{:?}", stage)` or an explicit match (stage enum is `Copy`, not `non_exhaustive`) |
| `OcError` | no (`#[non_exhaustive]`, not `Clone`) | `{ kind: &'static str, path: Option<String>, detail: String }`; match with a wildcard arm to `"other"` |
| `Topology`, `TopologyBlock` | no | mirror (values via `WireValue`) |
| `CatalogEntry` | no, but `CATALOG_JSON` is JSON already | serve `CATALOG_JSON` verbatim or `serde_json::from_str` it into our own structs |

Also: f64 values pass through JSON losslessly in Rust (`serde_json` writes shortest round-trip), but
NaN and the infinities fail. Never `unwrap` a `serde_json::to_string` on frame data.

## 8. CXF cheat sheet for `mm-graph`'s emitter

The engine accepts a subset of CXF (ASHRAE 231P control exchange format, JSON-LD). Authoritative
docs in the engine repo: `docs/cxf-composite-subset.md`, `docs/cxf-round-trip.md`. Accepted
fixtures: `crates/oce-cxf/tests/fixtures/` (for example `connector_attrs.jsonld` and
`composite_contract/`).

Minimal shape (a constant feeding a gain, from the engine's own doctest):

```json
{
  "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
  "@graph": [
    { "@id": "http://example.org#Example", "@type": "S231:Block", "S231:label": "Example",
      "S231:containsBlock": [ {"@id": "http://example.org#Example.con"}, {"@id": "http://example.org#Example.gain"} ],
      "S231:hasOutput": { "@id": "http://example.org#Example.y" } },
    { "@id": "http://example.org#Example.con",
      "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
      "S231:label": "con",
      "S231:hasParameter": { "@id": "http://example.org#Example.con.k" },
      "S231:hasOutput": { "@id": "http://example.org#Example.con.y" } },
    { "@id": "http://example.org#Example.con.k",
      "S231:value": { "@value": "2.0", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
    { "@id": "http://example.org#Example.con.y", "@type": "S231:RealOutput",
      "S231:isOfDataType": { "@id": "S231:Real" },
      "S231:isConnectedTo": { "@id": "http://example.org#Example.gain.u" } }
    /* ... gain block, its .k parameter, .u RealInput, .y RealOutput connected to Example.y,
       and the outer Example.y RealOutput ... */
  ]
}
```

Rules that bit people (all from engine docs and observed behavior):

- **Class IRI vs catalog path.** The block `@type` is `<base-iri>Buildings.Controls.OBC.<class_path>`
  (the engine reads the fragment after the base). `class_path` from `catalog()` is
  `CDL.Reals.Sources.Constant`. The emitter must map one to the other. Unknown paths give
  `class-not-found`.
- **Ports are separate nodes.** Each block has `S231:hasInput`, `S231:hasOutput`, `S231:hasParameter`
  referencing separate `@id` nodes: `<block>.<port>`. Inputs are typed
  `S231:RealInput` / `S231:BooleanInput` / `S231:IntegerInput` with `S231:isOfDataType`. A connection
  is written on the **source output** as `S231:isConnectedTo` -> target input `@id`.
- **Parameters** are bare nodes `<block>.<param>` with `S231:value {"@value": "<string>", "@type": xsd:double|integer|boolean}`.
  Values are strings in the JSON.
- **Top-level interface.** The root `S231:Block` lists `S231:containsBlock`, `S231:hasInput`,
  `S231:hasOutput`. The root's inputs and outputs are the frame boundary (`input_definitions()` and
  `CompletedFrame::outputs()`). The `@id` of those nodes is the point identity everywhere: frame
  keys, watch keys, BACnet name mapping, UI labels. Treat `@id`s as canonical and stable.
- **Inline `@context` only.** No remote context, no relative IRIs (`relative-iri`).
- **Array order is load-bearing** (schedule and port order derive from it). The emitter must be
  deterministic: sort by a stable key and never iterate a `HashMap` when writing arrays.
- **Limits.** 8 MiB document (`MAX_CXF_BYTES`), 64 nesting levels. Both raise typed errors.
- **Accepted means warning-free** for the composite subset: treat any warning in the load receipt as
  a failure of the draft (feed it back to the agent) unless the code is on an allowlist.
- **Port attribute spellings** (unit/range metadata) seen in fixtures: `S231:unit`,
  `S231:quantity`, `S231:displayUnit`, `S231:min`, `S231:max`. These show up in `PointInfo.unit`,
  `.quantity`, `.display_unit`, `.min`, `.max` and in `InputDefinition.min/max` (which then reject NaN).
- **Never load untrusted CXF in the process that commands equipment.** The model that came out of
  an LLM is untrusted input. Authoring and verification run in `mm-agent` / `mm-bridge` in a separate
  engine (and ideally a separate process) from the engine instance `mm-runtime` uses; `mm-runtime`
  accepts only a CXF that has a recorded green verification receipt (content id).
- Do not paste third-party CXF or ASHRAE text into the repo. Our emitter writes its own CXF from
  our graph JSON. Fixtures we ship are generated by our emitter.

## 9. Vector replay (what the library's verifier did, and what we must reimplement)

`open-control-library/faults/*/*/vectors.json` has the shape (schema `cxf-library/vectors/v1`):

```json
{ "schema": "cxf-library/vectors/v1", "clock": { "step_s": 60, "horizon_s": 1800 },
  "scenarios": [ { "name": "both_open", "inputs": { "htg_vlv_cmd": 20.0, "clg_vlv_cmd": 30.0 },
                   "expect": [ { "output": "yFault", "from_s": 0, "to_s": 840, "equals": false },
                               { "output": "yFault", "from_s": 960, "to_s": 1800, "equals": true } ] } ] }
```

Semantics of the old verifier (read from `tools/verify/src/main.rs`), restated for the frame API:

- **Inputs**: each scenario input is either a constant (event at t=0) or an array of
  `{ "t": <s>, "value": <v> }` steps. Events are sorted by `t` and applied hold-last: at each tick,
  every event with `event.t <= t` is applied before the step. Hold-last is *harness* behavior; the
  engine never holds. In the frame API you must keep a current-value table per input and submit the
  whole table every frame.
- **Name resolution**: a vector name resolves to a boundary point by exact path, else by a unique
  `.name` / `#name` suffix; zero or several matches is an error. Outputs resolve the same way, and
  declared boundary-output aliases from `topology().boundary_outputs` are searched too.
- **Unspecified inputs**: the old engine seeded unwritten inputs, but v0.2.0 requires every input.
  Decide and document one rule (recommended: error unless the scenario sets it, with an explicit
  `defaults` map in our own scenario format).
- **Clock**: `n_ticks = floor(horizon_s / step_s)`; ticks `k = 0..=n_ticks`, `t = k * step_s`. Reject
  non-finite or non-positive `step_s`, negative `horizon_s`, more than 1,000,000 samples per
  scenario or in total, or more than 512 scenarios (the old verifier's safety limits; keep them).
- **Engine per scenario**: build and load a fresh `Engine` for each scenario so stateful blocks
  start clean.
- **Checks**: for every step with `from_s <= t <= to_s`, compare the output to `equals`. Reals use
  `|got - expected| <= tolerance` (default 1e-9, per-expect `tolerance` field); booleans and
  integers compare exactly. Gaps between windows are "don't care" (alarm delays, ramp edges).
- **Result**: pass, fail (first violating `t`, expected, actual, window), or error (typed `OcError`
  or diagnostic code). The agent repair loop consumes exactly this record.

Reference run (AHU-0016 `both_open`, 60 s steps, through the snippet in section 12): output false
through t=840, true from t=900 (sequence numbers 1..=21). The vector's gap (841..959) exists precisely
because of that edge.

## 10. Gotchas (short list)

1. `Value` has no `PartialEq`/`Serialize`; use `bit_eq` and a DTO.
2. Every input every frame, correct type. No partial frames. String inputs do not exist.
3. `halt` -> `set_param` -> `resume` restarts the run (state reset). Not for live tuning.
4. Frame-prepared plans go stale on reload or resume; prepare right before execute.
5. A refused frame leaves the engine unchanged, but the plan is consumed. Re-prepare.
6. `OcError` and `Diagnostic*` types are not `Clone`/`Serialize` (mostly) and `OcError` is
   `#[non_exhaustive]`.
7. `get_output` reads latest state; use the `CompletedFrame` for anything you display as "step N".
8. `CompletedFrame::outputs()` are lexically ordered by path, not declaration order.
9. Catalog has elementary CDL only (136). No G36 composites; Trim and Respond is a composition.
10. Class IRI in CXF differs from `class_path`; map it.
11. The library repo's verifier targets a removed API (`ENGINE_PIN` mismatch). Do not depend on it.
12. NaN is legal for unbounded Real inputs and will propagate. Validate in the host.
13. `MemStore` only. No persistence across process restarts; use `state_snapshot()` bytes yourself if
    you ever need continuity (host-owned, not needed for the demo).
14. Engine instances are cheap. Build a fresh one for every verification run; keep one long-lived
    one for the runtime loop only.

## 11. Cargo/testing hooks that pay off

- Golden-test the emitter: graph JSON -> CXF bytes must be byte-identical (deterministic). Store
  goldens under `crates/mm-graph/tests/golden/` and update with an explicit
  `UPDATE_GOLDEN=1` env flag.
- Round-trip test: `load_cxf` the emitted bytes with the receipt, assert `records()` is empty, then
  `export_cxf()` and compare `content_id()` stability across two runs (determinism check).
- Replay `ReplayRecord::verify` for a short run to prove frame-level determinism in CI.

## 12. Verified snippet: load a CXF, replay a CSV of inputs, print outputs

Compiled and run against the pinned rev. Usage: `replay <model.cxf.jsonld> <inputs.csv> [max_frames]`.
CSV header `time,<input>,<input>...`; each column header is a canonical input path or a unique
`.name` / `#name` suffix of one.

```toml
[package]
name = "replay"
version = "0.0.0"
edition = "2024"
rust-version = "1.97.0"

[dependencies]
oce-api = { git = "https://github.com/jscott3201/open-control-engine", rev = "4063202b9a9e50c723bb858d786108571d3d401d" }
```

```rust
use std::error::Error;
use std::fs;

use oce_api::{Engine, Value, ValueType};

fn parse_value(ty: ValueType, raw: &str) -> Result<Value, Box<dyn Error>> {
    Ok(match ty {
        ValueType::Real => Value::Real(raw.parse()?),
        ValueType::Integer => Value::Integer(raw.parse()?),
        ValueType::Boolean => Value::Boolean(match raw {
            "1" | "true" | "TRUE" => true,
            "0" | "false" | "FALSE" => false,
            other => return Err(format!("not a boolean: {other}").into()),
        }),
        // EnumClassId is not re-exported, but the field and variant are usable without naming it.
        ValueType::Enum(class) => Value::Enum { class, ordinal: raw.parse()? },
        ValueType::String => return Err("String is metadata-only, never a boundary input".into()),
    })
}

fn show(value: &Value) -> String {
    match value {
        Value::Real(v) => format!("{v}"),
        Value::Integer(v) => v.to_string(),
        Value::Boolean(v) => v.to_string(),
        Value::String(v) => format!("{v:?}"),
        Value::Enum { class, ordinal } => format!("enum#{}:{ordinal}", class.0),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let cxf_path = args.next().ok_or("usage: replay <model.cxf.jsonld> <inputs.csv> [max_frames]")?;
    let csv_path = args.next().ok_or("missing inputs.csv")?;
    let max_frames: usize = args.next().map(|s| s.parse()).transpose()?.unwrap_or(usize::MAX);

    // 1. Load with typed diagnostics.
    let mut engine = Engine::in_memory();
    let receipt = engine
        .load_cxf_with_receipt(&fs::read(&cxf_path)?)
        .map_err(|failure| {
            for rec in failure.diagnostics().records() {
                eprintln!("{:?} {} {:?}: {}", rec.key().stage(), rec.key().code(),
                    rec.key().severity(), rec.message());
            }
            format!("load failed in stage {:?}: {failure}", failure.stage())
        })?;
    for rec in receipt.diagnostics().records() {
        eprintln!("warn {:?} {} {:?}: {}", rec.key().stage(), rec.key().code(),
            rec.key().severity(), rec.message());
    }
    eprintln!("loaded {} blocks ({} stateful), io {:?}", receipt.report().block_count,
        receipt.report().stateful_blocks, receipt.report().io);

    // 2. Map CSV columns -> required boundary inputs.
    let defs = engine.input_definitions()?;
    let text = fs::read_to_string(&csv_path)?;
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<&str> = lines.next().ok_or("empty csv")?.split(',').map(str::trim).collect();
    let mut col_for_input = Vec::with_capacity(defs.len());
    for def in &defs {
        let hits: Vec<usize> = header.iter().enumerate().skip(1)
            .filter(|(_, h)| **h == def.path
                || def.path.ends_with(&format!(".{h}")) || def.path.ends_with(&format!("#{h}")))
            .map(|(i, _)| i).collect();
        match hits.as_slice() {
            [i] => col_for_input.push(*i),
            _ => return Err(format!("input `{}`: {} matching csv columns", def.path, hits.len()).into()),
        }
    }

    // 3. One prepare + execute per row; time must be finite and nondecreasing.
    for (row, line) in lines.take(max_frames).enumerate() {
        let cells: Vec<&str> = line.split(',').map(str::trim).collect();
        let time: f64 = cells.first().ok_or("missing time")?.parse()?;
        let mut entries: Vec<(&str, Value)> = Vec::with_capacity(defs.len());
        for (def, &col) in defs.iter().zip(&col_for_input) {
            entries.push((def.path.as_str(), parse_value(def.value_type, cells[col])?));
        }
        let prepared = engine.prepare_frame(time, &entries)?; // refuses missing/extra/typed-wrong
        let frame = engine.execute_frame(prepared)?; // consumes the prepared frame
        let outs: Vec<String> = frame.outputs().iter().map(|(p, v)| format!("{p}={}", show(v))).collect();
        println!("row {row} t={} seq={} {}", frame.time(), frame.sequence(), outs.join(" "));
        for w in frame.diagnostics() {
            eprintln!("  warn t={} {}: {}", w.t, w.block, w.message);
        }
    }
    Ok(())
}
```

`ValueType` is `Copy`, so `def.value_type` can be read by value. `class.0` is public on `EnumClassId`.

Sample (AHU-0016 rule, `both_open`, 60 s steps): `yFault=false` through t=840, `true` from t=900.

## 13. Library compatibility note (important)

`open-control-library` ships `tools/verify` (crate `cxf-verify`) pinned to an engine commit
(`ENGINE_PIN`, rev `41e997f...`). That verifier drives the removed `tick` / `set_input` API and
cannot compile against v0.2.0. Do not add it as a dependency, do not try to update it (the library
is frozen for the event), and do not describe our harness as "the library verifier". `mm-bridge`
implements vector replay itself (section 9) and credits the vector format as pre-existing.

## 14. rusty-bacnet: serving engine points as BACnet objects

Crates (all at the pinned rev): `bacnet-objects` (object model), `bacnet-server` (B/IP server),
`bacnet-types` (enums, `ObjectIdentifier`, `PropertyValue`). MSRV 1.93 (at v0.14.0), so the engine's
1.97 floor covers it. Needs a Tokio runtime (`tokio = { version = "1", features = ["macros",
"rt-multi-thread", "time", "signal"] }`).

Object mapping from `PointInfo`:

| Engine point (`PointInfo.io_class` + `physical`) | BACnet object | How we update it each step |
|---|---|---|
| Sensor / read-only analog values from BOPTEST (zone temps, SAT, power) | `AnalogInputObject` | `server.set_present_value_local(&oid, PropertyValue::Real(v as f32))` |
| Computed values we publish (resets, setpoints chosen by the sequence) | `AnalogValueObject::with_access(.., PresentValueAccess::ReadOnly)` | `set_present_value_local` |
| Commanded outputs (valve/damper/supply setpoint commands) | `AnalogOutputObject` | `server.write_local(&oid, PRESENT_VALUE, None, PropertyValue::Real(v), Some(16), LocalCommandSource::ServerDevice)` |

Full verified example (compiles and runs against the pinned rev; loopback, ephemeral port):

```rust
use std::net::Ipv4Addr;

use bacnet_objects::DeviceIdentity;
use bacnet_objects::analog::{AnalogInputObject, AnalogOutputObject, AnalogValueObject};
use bacnet_objects::database::ObjectDatabase;
use bacnet_objects::device::{DeviceConfig, DeviceObject};
use bacnet_objects::present_value_access::PresentValueAccess;
use bacnet_server::LocalCommandSource;
use bacnet_server::mutation::MutationPolicy;
use bacnet_server::server::BACnetServer;
use bacnet_types::enums::{EngineeringUnits, ObjectType, PropertyIdentifier};
use bacnet_types::primitives::{ObjectIdentifier, PropertyValue};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Object database: one Device + points. Add every object BEFORE building the server.
    let identity = DeviceIdentity::new(260_001, 999)?.with_name("mechanical-muse"); // vendor 999 = placeholder
    let mut db = ObjectDatabase::new();
    db.add(Box::new(DeviceObject::new(DeviceConfig { identity: identity.clone(), ..DeviceConfig::default() })?))?;
    db.add(Box::new(AnalogInputObject::new(1, "zone1_temp", EngineeringUnits::DEGREES_CELSIUS)?))?;
    db.add(Box::new(AnalogValueObject::with_access(
        1, "sat_setpoint", EngineeringUnits::DEGREES_CELSIUS, PresentValueAccess::ReadOnly)?))?;
    db.add(Box::new(AnalogOutputObject::new(1, "chw_valve_cmd", EngineeringUnits::PERCENT)?))?;

    // 2. Serve on B/IP. Port 0 = ephemeral (tests); use 47808 (0xBAC0) for a real demo.
    let mut server = BACnetServer::bip_builder()
        .device(identity)
        .interface(Ipv4Addr::LOCALHOST)
        .port(0)
        .broadcast_address(Ipv4Addr::LOCALHOST)
        .mutation_policy(MutationPolicy::DenyAll) // read-only mirror: refuse all network writes
        .database(db)
        .build()
        .await?;

    // 3. Each control step: push values locally (engine f64 Real -> BACnet f32 REAL; finite only).
    let ai = ObjectIdentifier::new(ObjectType::ANALOG_INPUT, 1)?;
    let av = ObjectIdentifier::new(ObjectType::ANALOG_VALUE, 1)?;
    let ao = ObjectIdentifier::new(ObjectType::ANALOG_OUTPUT, 1)?;
    server.set_present_value_local(&ai, PropertyValue::Real(22.4_f64 as f32)).await?;
    server.set_present_value_local(&av, PropertyValue::Real(13.0_f64 as f32)).await?;
    // AO is commandable: set_present_value_local fails closed for it. Use a prioritised local write.
    server.write_local(&ao, PropertyIdentifier::PRESENT_VALUE, None,
        PropertyValue::Real(55.0), Some(16), LocalCommandSource::ServerDevice).await?;

    let read = server.read_local(&ao, PropertyIdentifier::PRESENT_VALUE, None).await?;
    println!("ao present value = {read:?}");   // Real(55.0)
    server.stop().await?; // always stop() before drop
    Ok(())
}
```

Observed results from running it: `read_local` on the AO returns `Real(55.0)`, on the AI `Real(22.4)`;
`set_present_value_local` on the AO returns an error (`OBJECT` /
`OPTIONAL_FUNCTIONALITY_NOT_SUPPORTED`); a NaN on an AI returns `PROPERTY` / `VALUE_OUT_OF_RANGE`.

rusty-bacnet notes:

- `set_present_value_local`, `write_local`, `read_local` all take `&self`, so wrap the server in an
  `Arc` and share it with the runtime task. `stop()` needs `&mut self`.
- Object instance numbers are per type (AI 1 and AO 1 can coexist). Names must be unique per
  device. Derive the name from the sanitized CXF `@id` fragment and keep a stable
  `HashMap<path, (ObjectIdentifier, kind)>` table in `mm-bacnet`; generate it from
  `engine.point_list(None)`.
- The Device identity vendor id `999` above is a placeholder, not a registered vendor id. Keep it as
  a clearly labelled constant (`PLACEHOLDER_VENDOR_ID`) and say so in the README.
- `MutationPolicy::DenyAll` refuses every covered network mutation (WriteProperty, CreateObject,
  etc.). Use it for the default read-only mirror. For an operator-override demo, use
  `MutationPolicy::Permissive` plus a `FnPolicy` (`.policy(...)`) that allows writes only to the
  override objects, and read them back with `read_local` each step. Do not enable network writes
  to the AOs the sequence owns.
- Values are `f32` on the wire. Quantize exactly once, in `mm-bacnet`, and compare against the
  engine's `f64` with a tolerance in tests.
- Updates run COV and intrinsic-event processing after the database lock is released. Subscribed
  clients get notifications for free.
- Engine outputs of `Boolean` type map to `BinaryInput` / `BinaryOutput` objects; check the
  `bacnet-objects::binary` module for the matching constructor and `set_present_value_local`
  accepting `PropertyValue::Enumerated`. This is untested here: verify before relying on it. Prefer
  analog-only points for the hero demo.
- Out_Of_Service on an object makes `set_present_value_local` refuse; do not set it.
- The server binds UDP. In docker compose with BOPTEST, run `mm-server` on the host network or map
  `47808/udp`. The BACnet broadcast address on a bridge network will not reach the host LAN, so
  demo with a client (YABE, `bacnet-stack`, or rusty-bacnet's own client) on the same host.

## 15. Reproducing the checks

```bash
# engine API snippet
cargo new --bin replay && cd replay   # paste Cargo.toml dep + main.rs from section 12
cargo run -- path/to/model.cxf.jsonld path/to/inputs.csv 30
# public API baseline in the pinned engine checkout:
#   crates/oce-api/tests/public-api.txt   (2,560 lines; this is the frozen surface)
# catalog dump
#   python3 -I -c "import json;print(len(json.load(open('crates/oce-api/contracts/catalog.json'))['entries']))"   # 136
```

When the pin moves (it should not during the event), regenerate `public-api.txt` in the engine
checkout and diff it against this reference before touching any code.
