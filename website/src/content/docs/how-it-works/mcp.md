---
title: MCP server for coding agents
description: mm-mcp exposes specs, the block palette, the validator, the CXF emitter and vector replay to coding agents over MCP.
---

`mm-mcp` is an MCP server on stdio. It lets a coding agent, such as Meta's Muse Code or any
other MCP client, author a control sequence the way our own agent does: read a spec, draft a
block graph, and have our deterministic tools check it. Here the agent in the client is the
muse.

The server makes no model calls and no network calls. Every verdict comes from the same code
our authoring loop runs, so the same graph always gives the same content id and result. The
protocol side is the official MCP Rust SDK, `rmcp` (pre-existing, pinned); the tools are ours.

## Connect

The repository's `.mcp.json` already lists the server as `mechanical-muse`. Build it once,
then start it from the repository root:

```sh
cargo build -p mm-mcp
make mcp                 # cargo run --quiet --locked -p mm-mcp
```

## Tools

| Tool | What it does |
| --- | --- |
| `list_specs` | Each spec's name, summary, typed inputs and outputs, scenario count |
| `get_spec` | The task text our agent receives, the authoring rules and the block-graph JSON Schema |
| `palette` | Authorable block classes from the pinned catalog, with ports and parameters |
| `validate_graph` | Fast structural findings, and interface problems against a spec |
| `emit_cxf` | The CXF text and engine content id |
| `verify_graph` | The full check: "passes X of Y scenarios", the stage and code, each scenario's result, and the repair message our agent would get |
| `run_ledger`, `compare_runs` | A recorded run's Scope 1 and 2 [ledger](/mechanical-muse/how-it-works/emissions/), and the difference between two runs of the same window |

`get_spec` shows scenario names but not expected values, and no tool returns a reference graph,
so the task stays a task. `verify_graph`'s feedback does name a failing expected value; a graph
repaired against it has been repaired against the scoring vectors and should be reported that
way.

## Learn more

- [docs/mcp.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/mcp.md):
  protocol revisions, error codes and an example session.
- [docs/coding-agents.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/coding-agents.md):
  how the repository is set up for Muse Code and Claude Code.
