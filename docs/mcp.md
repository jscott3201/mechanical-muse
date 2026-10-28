# mm-mcp: our tools for coding agents

`crates/mm-mcp` is an MCP server on stdio. It lets a coding agent, such as Meta's Muse Code or
any other MCP client, author a control sequence the way our own agent does: read a spec, draft a
compact block graph, and have our deterministic tools check it. The muse proposes, the engine
checks; here the agent in the client is the muse.

The server makes no model calls and no network calls. Every verdict comes from code our own
authoring loop already runs: the catalog palette and validator in `mm-graph`, the CXF emitter,
the engine load with typed diagnostics and the vector replay in `mm-bridge` (which uses the
pinned open-control-engine), and the repair text from `mm-agent`. The same graph always gives the
same content id and the same result.

## Connect

The repository's `.mcp.json` already lists the server as `mechanical-muse`; Muse Code and Claude
Code both read it once the workspace is trusted. It runs:

```sh
cargo run --quiet --locked -p mm-mcp     # or: make mcp
```

Build it once first (`cargo build -p mm-mcp`) so the client does not time out on the first
compile. The client must start it from the repository root (both harnesses do). Specs come from
`MM_SPECS_DIR`, else `specs/` in the working directory, else the repository's own `specs/`. Logs go
to stderr only (`RUST_LOG`, default `warn`); stdout carries only protocol messages.

Other clients: point them at the same command with the repository as working directory. In
Claude Code, `/mcp` shows the server's status; in Muse Code, the MCP list after trust.

## Protocol

The protocol side is the official MCP Rust SDK, [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk)
3.5.1 (pinned exactly; features `server` and `transport-io` only). Our code is the
`ServerHandler` in `src/handler.rs`: server identity and instructions, the tool list, and
`tools/call` routed to our tools (run on the blocking pool, since a replay is CPU-bound). The
SDK owns JSON-RPC framing, version negotiation, `initialize`, `server/discover`, `ping` and the
error replies.

What rmcp 3.5.1 does, as observed in our tests (`tests/stdio.rs`):

- **Revisions:** `2024-11-05`, `2025-03-26`, `2025-06-18`, `2025-11-25` and `2026-07-28`, the set
  the SDK knows (`ProtocolVersion::KNOWN_VERSIONS`); we do not narrow it.
- **Handshake clients** (`2025-11-25` and earlier): `initialize` answers with the client's
  revision when it has a handshake, else `2025-11-25`. Replies to these clients carry no
  `resultType`.
- **Per-request clients** (`2026-07-28`): no handshake is needed. `server/discover` lists the
  supported versions; a request whose `_meta` names another version gets `-32022` with the list.
  Results carry `resultType: "complete"`.
- An unknown tool gets `-32602`, an unknown method `-32601`. A line that is not JSON gets no
  reply and the session continues. Methods we do not implement but the SDK defaults (for example
  `resources/list`) return empty lists.

## Tools

Every result is JSON, in `structuredContent` and as the text block. A tool that cannot do its job
(missing argument, unknown spec, input that is not a block graph, a graph that cannot be emitted)
returns `isError: true` with `{code, message, details}`. A graph that fails verification is
an ordinary result with `passed: false` and a `code`. Codes are our stable strings: validator
findings (`unknown-class`, `undriven`, `type-mismatch`, ...), engine diagnostics
(`algebraic-loop`, `unresolved-reference`, ...), and `interface-mismatch`, `replay-failed`,
`emit-failed`, `not-a-graph`, `unknown-spec`, `invalid-arguments`.

| Tool | Arguments | Returns |
|---|---|---|
| `list_specs` | none | Each spec's name, title, summary paragraph, typed inputs and outputs, scenario count |
| `get_spec` | `spec` | The task text our agent receives, inputs, outputs, parameters, scenario names and descriptions, the authoring rules, the block-graph JSON Schema |
| `palette` | `query` (optional) | Authorable block classes from the pinned catalog, one line each with ports in order and parameters (`*` = required); the listing our agent sees, filtered by class name |
| `validate_graph` | `graph`, `spec` (optional) | `ok`, `code`, validator `findings`, and `interface_problems` against the spec |
| `emit_cxf` | `graph`, `spec` (optional) | CXF text, byte count and the engine content id; with a spec the graph takes the spec's name, as in `verify_graph` |
| `verify_graph` | `graph`, `spec` | `passed`, `summary` ("passes X of Y scenarios"), `stage` (interface, validate, emit, load, replay), `code`, `content_id`, every scenario's result (a failure gives the first failing time, output, expected and actual values and the window), and `feedback`, the repair message our agent would get (loop chains, inputs at the failing time) |
| `run_ledger` | `record` | A recorded run's Scope 1 (gas) and Scope 2 (electricity) energy, kgCO2e and USD, `reconciles`, `measured`, end uses when measured, discomfort KPIs ([emissions.md](emissions.md)) |
| `compare_runs` | `a`, `b` | Both ledgers and `delta` = b minus a; `window-mismatch` unless both cover the same window |

What the tools keep back: `get_spec` shows scenario names and descriptions but not expected
values, and no tool returns a spec's reference graph, so the task stays a task. `verify_graph`'s
feedback does name a failing scenario's expected value, as our agent's vector-feedback mode
does; a graph repaired against it has been repaired against the scoring vectors and should be
reported that way. Passing all scenarios means the graph passes these vectors; it is not a proof
that the sequence is correct.

### Ledger tools

`run_ledger` and `compare_runs` read recorded runs (`<name>.jsonl`) from `MM_REPLAYS_DIR`, else
`fixtures/replays` (found like the specs). They call the same code as the HTTP API's ledger routes
(`mm_server::ledger` over `mm_runtime::ledger`): the exact split of BOPTEST's energy and emission
KPIs into gas and electricity, re-priced against its cost KPI, and end uses from the recorded
power readings when the record has all fourteen. Records from before those readings were kept
give scope totals only (`measured: false`). The numbers are simulation results under BOPTEST's
factors and prices for this test case; see [emissions.md](emissions.md) for what they do and do
not cover. Codes: `unknown-record`, `no-kpis`, `window-mismatch`, `no-replays`,
`invalid-arguments`. `crates/mm-mcp/tests/ledger.rs` drives both through the binary.

## Example session

Driven by hand (`make mcp` and JSON lines on stdin; replies trimmed):

```text
-> {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"by-hand","version":"0"}}}
<- {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"mm-mcp","title":"Mechanical Muse","version":"0.1.0"},"instructions":"..."}}
-> {"jsonrpc":"2.0","method":"notifications/initialized"}
-> {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_specs","arguments":{}}}
<- specs: ahu_chw_requests (15 scenarios), ahu_static_pressure_reset (6), chw_plant_reset (10), zone_pressure_requests (9)
-> {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"palette","arguments":{"query":"Reals.Sub"}}}
<- {"count":1,"classes":["- Reals.Subtract: in(u1: Real, u2: Real) -> out(y: Real); params: none"]}
-> verify_graph on ahu_chw_requests with one delay halved
<- {"passed":false,"stage":"replay","code":"replay-failed","summary":"passes 10 of 15 scenarios",
    "feedback":"Your block graph was not accepted.\nIt loads, but 5 of 15 test scenarios failed:\n- three_requests (...): at t = 660 s, yChiWatResReq should be 0 (checked from 0 to 660 s) but was 3\n  Inputs at t = 660 s: TAirSup = 289.5 (since 600 s), ..."}
-> verify_graph on ahu_chw_requests with the delay restored
<- {"passed":true,"stage":"replay","code":null,"summary":"passes 15 of 15 scenarios","content_id":"cxf:fnv1a128:b96e789a34c3a401e6134400125afb7e","feedback":null}
```

## Tests

`crates/mm-mcp/tests/stdio.rs` starts the real binary on pipes, one process per test. It covers:

- a session: initialize, tools/list, then `verify_graph` on a reference graph that passes every
  scenario, on a mistimed graph that fails replay, and on a graph with an undriven output;
- an unknown class (`unknown-class` from `validate_graph`, a tool error from `emit_cxf`);
- emission that is byte-identical across two processes and matches `verify_graph`'s content id;
- spec and palette listing, `unknown-spec` and `not-a-graph`;
- a `2026-07-28` client with no handshake (discover, the `-32022` version error, a verify);
- protocol errors that leave the session running.
