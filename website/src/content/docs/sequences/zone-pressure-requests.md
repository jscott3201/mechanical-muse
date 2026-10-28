---
title: Zone static pressure requests
description: One VAV zone's requests for more duct static pressure, from its airflow shortfall and damper position.
---

A sequence for one VAV zone that tells its air handler when the duct static pressure is too low
for it. A zone whose airflow stays short of its setpoint asks for more pressure, harder the
shorter it is, and a nearly fully open damper asks too. The air handler sums its zones' requests
in the [static pressure reset](/mechanical-muse/sequences/ahu-static-pressure-reset/). It runs
once per zone: 15 zones, 5 per floor.

Written in our own words. Version 1 followed the Modelica Buildings Library's VAV terminal
system-request sequence; version 2 adapts it to BOPTEST.

## Why version 2

In version 1, airflow requests counted only once a damper had been above 95 % for a minute. On
BOPTEST the zone dampers did not open that far in time: at 50 Pa, airflow fell to 55 % of
setpoint within minutes while dampers crept from 0.57 to 0.77 over three hours. No zone ever
asked, and the reset trimmed to its floor while zones starved. Version 2 asks from the airflow
shortfall itself.

## Interface

| Point | Type | Meaning |
| --- | --- | --- |
| `uDam` | real input | Zone damper position, 0 to 1 |
| `VDis_flow` | real input, m³/s | Measured zone airflow |
| `VSet_flow` | real input, m³/s | Zone airflow setpoint |
| `yPreReq` | integer output | Static pressure reset requests, 0 to 3 |

## Behavior in brief

- With the zone served (setpoint above 0.01 m³/s), a shortfall that has lasted 120 s sends
  **3 requests** below half the setpoint, else **2** below 70 %, else **1** below 90 %. Each
  shortfall has a 0.01 m³/s release band.
- A damper above 95 % open (held until no longer above 85 %) sends at least **1 request** at once.
- Every threshold is strict; a shortfall present at the first frame still waits the full 120 s.

With two requests ignored, a floor's reset responds once three or more of its five zones fall
below 90 % of setpoint.

## Validation

- 9 scenarios from the independent reference model: damper band edges, each request level,
  airflow recovering through the bands, an unserved zone, brief dips, damper and airflow together.
- A 23-block hand-built graph passes all 9; 10 deliberate mistakes each fail at least one.

## How it did

- Muse Spark (version 2): first try 0 of 3, after repair 3 of 3. Every first draft missed the
  same edge (a damper at exactly 85 % must release), each fixed in one repair.
- Version 1 on BOPTEST saved 33.6 % energy by under-ventilating (+51.4 % CO2 discomfort);
  version 2 settles near 165 to 215 Pa. See [Runtime results](/mechanical-muse/results/runtime/#duct-static-pressure-reset).

Full spec: [specs/zone_pressure_requests/spec.md](https://github.com/jscott3201/mechanical-muse/blob/main/specs/zone_pressure_requests/spec.md)
