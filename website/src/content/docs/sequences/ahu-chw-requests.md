---
title: Air handler chilled-water requests
description: One air handler's chilled-water reset requests (0 to 3) and chiller plant request, from supply-air error and cooling valve position.
---

A sequence for one air handler that tells the chilled-water plant how hard it is working. Its
reset requests (0 to 3) are summed over the air handlers and feed the
[plant reset](/mechanical-muse/sequences/chw-plant-reset/); its plant request (0 or 1) says the
air handler needs the chillers.

Written in our own words from the behavior of the Modelica Buildings Library's multizone air
handler plant-request block. Only the chilled-water half is covered: BOPTEST's air handlers have
no hot-water coil.

## Interface

| Point | Type | Meaning |
| --- | --- | --- |
| `TAirSup` | real input, K | Measured supply air temperature |
| `TAirSupSet` | real input, K | Supply air temperature setpoint |
| `uCooCoi` | real input, 0 to 1 | Cooling coil valve position |
| `yChiWatResReq` | integer output | Chilled-water reset requests, 0 to 3 |
| `yChiPlaReq` | integer output | Chiller plant request, 0 or 1 |

## Behavior in brief

The error is supply air minus setpoint (positive when the air is too warm).

- **3 requests** when the error has been above 3 K for 120 s (held while above 2.9 K).
- **2 requests** when it has been above 2 K for 120 s (held while above 1.9 K).
- **1 request** while a valve latch is set: it sets above 0.95 open and clears below 0.85.
- **0** otherwise; the highest count wins.
- **Plant request** while a second latch is set: it sets above 0.95 and clears only below 0.10.

Every threshold is strict, and the task spells out the edge instants (a condition already met at
the first frame, a latch clearing and setting in one frame).

## Validation

- 15 scenarios from the independent reference model, with supply temperatures chosen so every
  error is exact in binary floating point.
- An 18-block hand-built graph passes all 15; 22 deliberate mistakes (wrong thresholds, `>=` for
  `>`, missing release bands, wrong priority, latches clearing at the wrong point) each fail at
  least one.

## How it did

- Muse Spark: 3 of 3 runs passed on the first try. An easy spec for the model: the task states
  every edge, and the sequence maps onto elementary blocks one to one.
- Runs live on BOPTEST, one instance per floor, feeding the plant reset.

Full spec: [specs/ahu_chw_requests/spec.md](https://github.com/jscott3201/mechanical-muse/blob/main/specs/ahu_chw_requests/spec.md)
