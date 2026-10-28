---
title: Duct static pressure reset
description: Trim and respond on one air handler's duct static pressure setpoint, fed by its zones' requests.
---

A supervisory sequence for one air handler. It lowers the supply duct static pressure setpoint
while the zones it serves are satisfied, and raises it when they report starved airflow. A lower
static pressure lets the supply fan slow down; BOPTEST's own controls hold every floor at a
constant 400 Pa.

Written in our own words from the behavior of the Modelica Buildings Library's generic trim and
respond block as its multizone air handler supply fan sequence uses it. The parameter values are
our choices for BOPTEST. It runs once per floor, fed by that floor's five
[zone request](/mechanical-muse/sequences/zone-pressure-requests/) sequences.

## Interface

| Point | Type | Meaning |
| --- | --- | --- |
| `nReqPre` | integer input | Static pressure reset requests from the zones on this air handler |
| `u1FanOn` | boolean input | The supply fan is running |
| `dpSet` | real output, Pa | Duct static pressure setpoint |

## Behavior in brief

- **Fan off:** `dpSet` = 400 Pa, at once.
- **Start delay:** after the fan starts, 400 Pa for 600 s plus one 120 s update period.
- **Updates** on a 120 s grid: trim by −12 Pa, or, when requests exceed 2, respond by +15 Pa per
  extra request (capped at +32 Pa). The setpoint stays within [50, 400] Pa.
- A stopped fan resets the stored setpoint.

## Validation

- 6 scenarios from the independent reference model: start delay and trim down to the floor,
  ignore/respond/cap, the ceiling, saturation and release, request changes between updates, a fan
  stop and restart.
- A 20-block hand-built graph passes all 6; 11 deliberate mistakes each fail at least one.

## How it did

- Muse Spark: 3 of 3 first try (high effort, with the loop rule in the prompt).
- On BOPTEST with zone requests v2 and the plant reset, 10 hours: energy −24.1 %, thermal
  discomfort −42.1 %, CO2 discomfort +17.4 %, on one window it was tuned on. See
  [Runtime results](/mechanical-muse/results/runtime/#duct-static-pressure-reset).

Full spec: [specs/ahu_static_pressure_reset/spec.md](https://github.com/jscott3201/mechanical-muse/blob/main/specs/ahu_static_pressure_reset/spec.md)
