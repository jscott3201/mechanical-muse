---
title: Sequences
description: Our own control sequence specs, each validated with an independent reference model and deliberately broken graphs before any model sees it.
---

Each sequence lives in its own folder under
[`specs/`](https://github.com/jscott3201/mechanical-muse/tree/main/specs), written in our own
words. Every spec has the same parts:

| File | What it is |
| --- | --- |
| `spec.md` | The sequence in prose: interface, parameters, behavior, edge instants, validation |
| `task.json` | The task Muse Spark receives |
| `reference.py` | An independent model of the behavior (stdlib Python, no engine code) that self-checks against hand-worked checkpoints and writes the vectors |
| `vectors.json` | The test scenarios a draft must pass. Generated, never hand-edited; CI regenerates them byte for byte |
| `graph.json` | A hand-built reference block graph |

## Validated before any model sees it

A spec is ready only when its hand-built graph passes every scenario on the pinned engine **and**
each of a list of deliberate mistakes fails at least one scenario. That second check is what
makes the vectors strong enough to catch wrong drafts.

| Sequence | Runs on | Scenarios | Reference graph | Mistakes caught |
| --- | --- | --- | --- | --- |
| [Chilled-water plant reset](/mechanical-muse/sequences/chw-plant-reset/) | BOPTEST office, plant | 10 | 33 blocks | 12 |
| [Air handler chilled-water requests](/mechanical-muse/sequences/ahu-chw-requests/) | BOPTEST office, one per air handler | 15 | 18 blocks | 22 |
| [Duct static pressure reset](/mechanical-muse/sequences/ahu-static-pressure-reset/) | BOPTEST office, one per floor | 6 | 20 blocks | 11 |
| [Zone static pressure requests](/mechanical-muse/sequences/zone-pressure-requests/) | BOPTEST office, one per zone | 9 | 23 blocks | 10 |
| [Home price response](/mechanical-muse/sequences/home-price-response/) | BOPTEST home heat pump | 22 | 33 blocks | 21 |

The four office sequences chain together: zone requests feed each floor's static pressure
reset, and air handler requests feed the plant reset.

Your own sequence can go through the same gate from the
[workbench](/mechanical-muse/how-it-works/workbench/), with scenarios you accept instead of a
reference model.
