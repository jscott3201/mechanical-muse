---
title: Spark checks its own work
description: Optional function tools that let Muse Spark run our checks on its draft before answering. Helpful, never a substitute for the receipt.
---

An authoring session can let Muse Spark call three of our checks on its own draft before it
answers. They are function tools on the Responses endpoint. They help Spark draft; they are not
verification. Every final draft still goes through the full
[receipt](/mechanical-muse/how-it-works/verification/).

| Tool | What it runs | What Spark gets back |
| --- | --- | --- |
| `check_graph` | Interface, validator, emitter, engine load | OK, or the stage and typed problems |
| `replay_scenario` | The checks above, then one named scenario on a fresh engine | Pass, or the first failing instant with expected and actual values |
| `describe_block` | The pinned catalog palette | A class's ports, parameters and whether it keeps state |

## Safeguards

- Tool arguments are model output: each is parsed and checked (size limit, shape, a known
  scenario or class), and a bad one comes back as a refusal Spark can read.
- Budgets: 8 tool turns per attempt, 24 tool calls per session, at most 4 of them
  `describe_block`.
- **Never in benchmark runs.** `replay_scenario` would show the scoring vectors to the model, so
  `mm-bench` never offers tools, and a test checks that.
- Off by default. The web app has a switch, and shows Spark's tool calls in a dashed box
  labelled as tool calls, not the receipt.

## A small live A/B (2026-10-27)

Two specs, two rounds, one session each way per round (`muse-spark-1.3`, high effort, up to 5
attempts):

- Chilled-water plant reset: with tools 2 of 2 verified; without, 0 of 2.
- Workbench revision B: with tools 2 of 2 verified on the first try; without, 1 of 2 (the other
  stopped on a content-filter refusal).
- Tool sessions used 6 to 50 times the input tokens.

This is a handful of sessions, not a benchmark. Tools look like they help on the harder spec, at
a real token cost, which is why they stay off by default.

## Learn more

- [docs/spark-tools.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/spark-tools.md):
  the full table of sessions, tokens and wall times.
