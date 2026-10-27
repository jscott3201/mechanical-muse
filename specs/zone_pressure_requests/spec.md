# Zone static pressure requests

A sequence for one VAV zone that tells its air handler when the duct static pressure is too low
for it: a damper that is nearly fully open asks for more pressure, and a zone whose airflow stays
well short of its setpoint with the damper open asks harder. The air handler sums its zones'
requests and resets its static pressure setpoint from them (`specs/ahu_static_pressure_reset/`).

Written in our own words from the behaviour of the static pressure half of the Modelica Buildings
Library's VAV terminal system-request sequence
(`Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/SystemRequests.mo`, as vendored
by the pinned open-control-engine). Nothing here is copied from it. It runs once per zone of
BOPTEST `multizone_office_complex_air` (15 zones, 5 per floor), from the zone's damper position,
airflow and airflow setpoint.

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
| damper open | 0.95 | Position above which the damper counts as open |
| damper band | 0.10 | It stays open until no longer above 0.85 |
| open time | 60 s | The damper must stay open this long before airflow requests count |
| served flow | 0.01 m³/s | Airflow setpoint above which the zone counts as served |
| served band | 0.005 m³/s | It stays served until the setpoint is no longer above 0.005 m³/s |
| three fraction | 0.5 | Airflow below half its setpoint: 3 requests |
| two fraction | 0.7 | Airflow below 70 % of its setpoint: 2 requests |
| flow band | 0.01 m³/s | Release band on the starved-airflow conditions |

## Behaviour

1. The damper is open once its position goes above 0.95, and stays open until its position is
   no longer above 0.85. While open, the zone sends at least 1 request.
2. Once the damper has stayed open for 60 s and the zone is served (setpoint above 0.01 m³/s,
   held down to 0.005), the zone sends 3 requests while its airflow is starved below half its
   setpoint, else 2 while starved below 70 % of it.
3. Starved below a fraction means (fraction × setpoint − airflow) has gone above 0; it holds
   until that difference is no longer above −0.01 m³/s.

Edge instants, stated in the task: every threshold is strict; the open time restarts at every
opening; a damper already open at the first frame still waits the full open time (sending 1
meanwhile).

## Deviations from the library block

- No samplers on the inputs: the air handler's reset samples the summed requests at its own
  updates, which fall on the runtime's step.
- The hysteresis bands are ours (the library leaves the airflow and setpoint bands to the
  designer).

## Validation (done before any model sees this spec)

- `reference.py` is an independent model of the behaviour (stdlib Python, no engine code). Its
  self-check compares it with checkpoints worked out by hand. It writes `vectors.json`: 7
  scenarios covering a part-open damper, the damper band at its exact edges, 3 and 2 requests,
  airflow recovering through both bands and falling back, a zone that is not served, and an
  opening that restarts the open time.
- `graph.json` is a hand-built block graph (17 blocks).
  `crates/mm-bridge/tests/specs_static_pressure.rs` checks that it passes every scenario on the
  pinned engine, and that each of these mistakes fails at least one: no damper band, a 90 %
  open threshold, a first-frame open passed at once, a 120 s open time, airflow requests without
  the open time, airflow requests without the served check, the 3 and 2 fractions swapped, no
  airflow bands, the single request only after the open time, and airflow compared with the
  whole setpoint.
