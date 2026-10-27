# Duct static pressure reset

A supervisory sequence for one air handler that lowers the supply duct static pressure setpoint
while the zones it serves are satisfied, and raises it again when they report starved airflow.
A lower static pressure lets the supply fan slow down; BOPTEST's own controls hold every floor's
setpoint at a constant 400 Pa.

Written in our own words from the behaviour of the Modelica Buildings Library's generic
trim-and-respond block as its multizone AHU supply fan sequence uses it
(`Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo` and `AHUs/MultiZone/VAV/SetPoints/SupplyFan.mo`,
as vendored by the pinned open-control-engine). Nothing here is copied from either; the
parameter values below are our choices for BOPTEST, with the library's trim, respond and cap
amounts. It runs once per floor of BOPTEST `multizone_office_complex_air`, fed by the five zone
request sequences of that floor (`specs/zone_pressure_requests/`).

## Interface

| Point | Type | Meaning |
| --- | --- | --- |
| `nReqPre` | integer input | Static pressure reset requests from the zones on this air handler |
| `u1FanOn` | boolean input | The supply fan is running |
| `dpSet` | real output, Pa | Duct static pressure setpoint |

## Parameters

| Name | Value | Meaning |
| --- | --- | --- |
| initial setpoint | 400 Pa | While the fan is off and during the start delay; equals BOPTEST's constant |
| lowest setpoint | 50 Pa | BOPTEST's lower bound for this setpoint |
| highest setpoint | 400 Pa | Never above BOPTEST's own value |
| start delay | 600 s | Wait after the fan starts before the first update period begins |
| update period | 120 s | Updates fall on multiples of this from time 0; equals the runtime step |
| ignored requests | 2 | Request counts up to this do not count |
| trim | −12 Pa | Change per update when not responding |
| respond | +15 Pa | Added per request above the ignored count |
| respond cap | +32 Pa | Most the respond part may add in one update |

## Behaviour

1. While the fan is off, `dpSet` = 400 Pa, at once.
2. After the fan starts, `dpSet` stays at 400 Pa for the start delay plus one update period
   (720 s). The delay starts again at every fan start.
3. Updates happen on the grid of multiples of 120 s. At each update the request count is read.
   If it exceeds 2, the change is the trim plus 15 Pa per extra request, with the respond part
   capped at 32 Pa; otherwise the change is the trim. The new setpoint is clamped to
   [50, 400] Pa.

Edge instants, stated in the task: with the fan on from 0 s the first change lands at the 720 s
update; requests that change at an update instant count at that update, and a count that rises
and falls between two updates is never used; the carried setpoint only changes at updates, so
the feedback path needs a sampled store; a stopped fan also resets that store.

## Deviations from the library block

- No device-status input beyond the fan: the library also resets on loss of the AHU's enable.
- Time is the engine's host tick (60 s steps in the vectors). Fan starts in the vectors fall on
  the update grid; request changes also fall between updates (`off_grid_requests`).

## Validation (done before any model sees this spec)

- `reference.py` is an independent model of the behaviour (stdlib Python, no engine code). Its
  self-check compares it with checkpoints worked out by hand. It writes `vectors.json`: 6
  scenarios covering the start delay and trim cadence down to the floor, ignore/respond/cap,
  the ceiling, saturation then release, request changes between updates (including a pulse no
  update sees), and a fan stop and restart.
- `graph.json` is a hand-built block graph (20 blocks).
  `crates/mm-bridge/tests/specs_static_pressure.rs` checks that it passes every scenario on the
  pinned engine, and that each of these mistakes fails at least one: start delay without the
  extra period, ignore count compared with "at least", uncapped respond, wrong respond amount,
  wrong trim, no floor, no ceiling, a restart that resumes from the trimmed value, requests used
  without sampling, a 300 s update period, and an initial setpoint at the floor.
