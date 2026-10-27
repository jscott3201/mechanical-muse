# Zone static pressure requests

A sequence for one VAV zone that tells its air handler when the duct static pressure is too low
for it: a zone whose airflow stays short of its setpoint asks for more pressure, harder the
shorter it is, and a damper that is nearly fully open asks too. The air handler sums its zones'
requests and resets its static pressure setpoint from them (`specs/ahu_static_pressure_reset/`).

Written in our own words. Version 1 followed the static pressure half of the Modelica Buildings
Library's VAV terminal system-request sequence
(`Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/SystemRequests.mo`, as vendored
by the pinned open-control-engine); version 2 adapts it to BOPTEST (below). Nothing here is
copied from the library. It runs once per zone of BOPTEST `multizone_office_complex_air` (15
zones, 5 per floor), from the zone's damper position, airflow and airflow setpoint.

## Why version 2

In v1, as in the library, airflow requests counted only once the damper had been above 95 % for
a minute: the zone controller is expected to open its damper fully when airflow falls short. On
BOPTEST it does not do that in time. With every floor's static pressure held at 50 Pa, zone
airflow fell to 55 % of setpoint within minutes while the dampers crept from 0.57 to 0.77 over
three hours (a direct BOPTEST probe, 2026-10-27). So no zone ever asked, and the reset trimmed to
its floor while zones starved: a 10-hour run saved 34 % energy against BOPTEST's baseline with
51 % more CO2 discomfort. v2 asks from the airflow shortfall itself, which BOPTEST's zones do show.
v1's task and vectors are kept (`task.v1.json`, `vectors.v1.json`) so its runs can be reproduced.

## Interface

| Point | Type | Meaning |
| --- | --- | --- |
| `uDam` | real input | Zone damper position, 0 to 1 |
| `VDis_flow` | real input, m³/s | Measured zone airflow |
| `VSet_flow` | real input, m³/s | Zone airflow setpoint |
| `yPreReq` | integer output | Static pressure reset requests, 0 to 3 |

## Parameters

| Name | Value | Meaning |
| --- | --- | --- |
| one fraction | 0.9 | A lasting shortfall below 90 % of setpoint: 1 request |
| two fraction | 0.7 | Below 70 %: 2 requests |
| three fraction | 0.5 | Below half: 3 requests |
| flow band | 0.01 m³/s | Release band on the shortfall conditions |
| persistence | 120 s | How long a shortfall must last before it counts (one runtime step) |
| served flow | 0.01 m³/s | Airflow setpoint above which the zone counts as served |
| served band | 0.005 m³/s | It stays served until the setpoint is no longer above 0.005 m³/s |
| damper open | 0.95 | Position above which the damper counts as open |
| damper band | 0.10 | It stays open until no longer above 0.85 |

## Behaviour

1. The airflow is short of a fraction of its setpoint once (fraction × setpoint − airflow) has
   gone above 0; it holds until that difference is no longer above −0.01 m³/s.
2. With the zone served (setpoint above 0.01 m³/s, held down to 0.005), a shortfall that has
   lasted 120 s sends 3 requests below half the setpoint, else 2 below 70 %, else 1 below 90 %.
3. A damper above 95 % open (held until no longer above 85 %) sends at least 1 request at once.

With five zones per floor and two requests ignored, a floor's reset responds once three or more
zones are below 90 % of their airflow setpoint, so it trims until zones begin to fall short.

Edge instants, stated in the task: every threshold is strict; each persistence restarts when
its condition turns on again; a shortfall already present at the first frame still waits the
full 120 s.

## Deviations from the library block

- Airflow requests do not wait for the damper to be fully open (why version 2), and a 90 %
  level sends 1 request; the 70 % and half levels and the damper request are the library's.
- No samplers on the inputs: the air handler's reset samples the summed requests at its own
  updates, which fall on the runtime's step.
- The hysteresis bands and the persistence are ours.

## Validation (done before any model sees this spec)

- `reference.py` is an independent model of the behaviour (stdlib Python, no engine code). Its
  self-check compares it with checkpoints worked out by hand. It writes `vectors.json`: 9
  scenarios covering a part-open damper, the damper band at its exact edges, 3, 2 and 1
  requests, airflow recovering through all three bands and falling back, a zone that is not
  served, brief dips that never last, and the damper and airflow requests together.
- `graph.json` is a hand-built block graph (23 blocks).
  `crates/mm-bridge/tests/specs_static_pressure.rs` checks that it passes every scenario on the
  pinned engine, and that each of these mistakes fails at least one: no damper band, a 90 %
  damper threshold, no damper request, a first-frame shortfall passed at once, a 60 s
  persistence, the 1-request level at 85 %, the 3 and 2 fractions swapped, no served check, no
  airflow bands, and airflow compared with the whole setpoint.
