---
title: Chilled-water plant reset
description: Trim and respond on the chilled-water supply temperature and differential pressure setpoints, with a staging hold.
---

The hero sequence. It relaxes the chilled-water plant while the air handlers are satisfied and
tightens it again when they ask for more cooling. One reset value `r` between 0 and 1 drives two
setpoints: at `r` = 1 the plant runs at design; as `r` falls the supply water warms first, and
only then does the pressure setpoint drop.

Written in our own words from the behavior of the Modelica Buildings Library's plant reset
sequence. Nothing is copied from it.

## Interface

| Point | Type | Meaning |
| --- | --- | --- |
| `nReqRes` | integer input | Number of chilled-water reset requests from the air handlers |
| `u1Ena` | boolean input | The plant is enabled |
| `u1StaPro` | boolean input | A chiller staging change is in progress |
| `TSupSet` | real output, K | Chilled-water supply temperature setpoint |
| `dpSet` | real output, Pa | Chilled-water differential pressure setpoint |

## Behavior in brief

- **Disabled:** `r` = 1, both setpoints at design (278.71 K, 239,125 Pa, BOPTEST's baseline
  values), at once.
- **Start delay:** after an enable, `r` holds at 1 for 900 s plus one 300 s update period.
- **Updates** on a 300 s grid: trim by −0.02, or, when requests exceed 2, respond by +0.03 per
  extra request (capped at +0.07). `r` stays within [0, 1].
- **Staging hold:** a staging change freezes updates for at least 900 s, longer if staging lasts.
- **Mapping:** supply temperature runs from 282.15 K at `r` ≤ 0.5 to design at `r` = 1; pressure
  runs from 95,650 Pa at `r` = 0 to design at `r` ≥ 0.5.

The task also states the edge instants (when the first change lands, which update a staging
start freezes), because the prose alone leaves them open.

## Validation

- 10 scenarios from the independent reference model: start delay and trim cadence,
  ignore/respond/cap, saturation and release, short and long staging holds, a second staging
  start, changes between updates, disable and re-enable, recovery from the floor.
- A 33-block hand-built graph passes all 10; 12 deliberate mistakes (such as a hold whose false
  duration keeps its default, or uncapped respond) each fail at least one.

## How it did

- Muse Spark: 3 of 3 live sessions verified with loop-aware repair and the loop rule; harder
  without vector feedback. See [Benchmark results](/mechanical-muse/results/benchmark/#control-sequences-our-own-specs).
- On BOPTEST with air handler requests, 10 hours: HVAC energy −4.7 %, chiller energy −14.2 %,
  thermal discomfort +2.0 %. See [Runtime results](/mechanical-muse/results/runtime/).

Full spec: [specs/chw_plant_reset/spec.md](https://github.com/jscott3201/mechanical-muse/blob/main/specs/chw_plant_reset/spec.md)
