# Muse Spark checks its own work (tool-assisted drafting)

An authoring session can let Muse Spark call three of our checks on its own draft before it
answers. They are function tools on the Responses endpoint (`docs/model-api.md` section 10 has
the wire findings). The tools only help Spark draft. They are not verification: every final
draft still goes through the same receipt as before (interface, validator, emitter, engine
load, then every scenario replayed on a fresh engine, with a content id). AGENTS.md hard rule 5
is unchanged.

## The tools

| Tool | Arguments | What it runs | What Spark gets back |
|---|---|---|---|
| `check_graph` | `graph` | interface against the task, `mm-graph` validator, CXF emitter, engine load | ok, or the stage and typed problems (codes, subjects; loop members as a chain) |
| `replay_scenario` | `graph`, `scenario` (an enum of the task's accepted scenario names) | the checks above, then `mm-bridge` replay of that one scenario on a fresh engine | pass, or the first failing instant: output, expected, actual, check window, inputs at that instant |
| `describe_block` | `class` | the pinned catalog palette | ports in order with types, parameters (required, type), whether it keeps state |

Code: `crates/mm-agent/src/tools.rs` (the checks), `tool_loop.rs` (one attempt's tool turns),
`repair.rs` (`author_with_tools`). The checks are the same functions the receipt and the MCP
server (`mm-mcp`) use (validator, emitter, load and replay), but only the receipt decides: a
`check_graph` "ok" is not a verdict on the draft. The checks are deterministic.

Safeguards:
- Arguments are model output. Each is parsed and checked (size limit 256 KB, JSON object, the
  block-graph shape, a known scenario or class); a bad one comes back as a refusal Spark can read.
- Outputs are capped at 6,000 bytes and list at most 12 problems.
- Budgets (`ToolLimits`): 8 tool turns per attempt, 24 tool calls per session, of which 4 may be
  `describe_block`. When a budget runs out, the open calls are answered "no more tool calls" and
  one last call without tools asks for the graph.
- Only product specs and workbench tasks get tools. A benchmark run never does: `replay_scenario`
  would show the library's scoring vectors to the model. `mm-bench` calls `author`, never
  `author_with_tools`; `crates/mm-bench/tests/no_tools.rs` checks every bench request and the
  bench source.

## Server and web

`POST /api/specs/{name}/author` takes `tools: true` (default false). Each of Spark's checks is a
`tool-call` event (`name`, a short argument summary, `ok`, a one-line `summary`); each checked
attempt also carries its model calls and its tool calls. The web authoring panel has a switch,
"Let Spark check its work (tools)", and lists Spark's checks inside each attempt in a dashed box
labelled as its tool calls, not the receipt. The attempt's verdict below it is the receipt.

Recordings keep tool turns (tool name and arguments; no call ids, no reasoning) and play back
without a key: the checks run again on this server. A recorded session with tools reads
`fixtures/recordings/<spec>.tools.json`; the committed one is a live plant-reset session (red,
then green). Older recordings play as before.

## A/B, live (2026-10-27)

`make live-tools-ab` (`crates/mm-server/tests/tools_ab_live.rs`): sessions through the router
on `muse-spark-1.3`, effort high, up to 5 attempts, repair messages naming failing scenarios.
Two specs: the chilled-water plant reset (10 scenarios) and revision B of the workbench sample
sheet (6 accepted scenarios). One session each way per round, two rounds. Before round 2 the
`describe_block` cap of 4 was added: in round 1 Spark spent 20 of its 24 tool calls on lookups
of classes the prompt already lists.

| Spec | Round | Tools | Result | Attempts | Model calls | Tool calls | Input tokens | Output tokens | Wall |
|---|---|---|---|---|---|---|---|---|---|
| CHW plant reset | 1 | no | not verified | 5 (refused, 8/10, cut off, 8/10, content filter) | 5 | 0 | 65,859 | 46,115 | 297 s |
| CHW plant reset | 1 | yes | verified on attempt 5 | 5 | 12 | 24 | 430,950 | 84,866 | 300 s |
| CHW plant reset | 2 | no | not verified | 5 (one cut off) | 5 | 0 | 121,446 | 53,938 | 301 s |
| CHW plant reset | 2 | yes | verified on attempt 2 | 2 | 18 | 20 | 795,895 | 70,163 | 223 s |
| Revision B | 1 | no | stopped (attempt 2 refused by the content filter) | 2 | 2 | 0 | 3,372 | 9,378 | 39 s |
| Revision B | 1 | yes | verified on attempt 1 | 1 | 6 | 12 | 76,457 | 13,714 | 41 s |
| Revision B | 2 | no | verified on attempt 1 | 1 | 1 | 0 | 3,372 | 6,902 | 21 s |
| Revision B | 2 | yes | verified on attempt 1 | 1 | 9 | 17 | 164,371 | 20,876 | 59 s |

"Cut off" is a reply that hit the 32,000-token output cap (reasoning counts against it); "content
filter" is an HTTP 400 saying the prompt triggered the provider's content policy, which ends a
session. Both happened only in sessions without tools here; with two sessions a side we can't
say whether that is chance. The lookup cap also changed between the two rounds (see above).

What we read from it, with four sessions a side:
- CHW plant reset: with tools, 2 of 2 verified; without, 0 of 2. This spec is hard for Spark
  without tools (the committed plain recording needed three attempts).
- Revision B: with tools, 2 of 2 verified first try; without, 1 of 2 (the other stopped on the
  content filter, so it says nothing about drafting).
- Cost: tools sessions used 6 to 50 times the input tokens (every tool turn re-sends the history,
  and each `check_graph` or `replay_scenario` call re-sends the whole draft as arguments) and
  more output tokens on revision B. Prefix caching covered part of the input. Wall time was
  similar or shorter on the plant reset (fewer wasted attempts) and the same or about three
  times longer on revision B.
- Spark's own checks are not the receipt. In the committed session, attempt 1's tool replays
  passed six scenarios, then it ran out of tool turns before replaying the other four (Spark
  replays one scenario per turn, so 8 turns cover about six of ten); the receipt then failed that
  draft and attempt 2 fixed it.

This is a handful of sessions, not a benchmark. Tools look like they help on the harder spec, at
a real token cost. They stay off by default.
