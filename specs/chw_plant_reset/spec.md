# Chilled-water plant reset

A supervisory sequence that relaxes the chilled-water plant while the air handlers are
satisfied, and tightens it again when they ask for more cooling. It drives two setpoints from
one reset value `r` between 0 and 1: at `r` = 1 the plant runs at design (coldest supply water,
highest differential pressure); as `r` falls the supply water is allowed to warm first, and only
then is the pressure setpoint lowered.

Written in our own words from the behaviour of the Modelica Buildings Library's plant reset
sequence (`Templates/Plants/Controls/Setpoints/PlantReset.mo` at commit `a131864e`) and the
plant reset intent in the 2024 Guideline 36 addendum drafts. Nothing here is copied from either.
This is the demo's hero sequence on BOPTEST `multizone_office_complex_air`.

## Interface

| Point | Type | Meaning |
| --- | --- | --- |
| `nReqRes` | integer input | Number of chilled-water reset requests from the air handlers |
| `u1Ena` | boolean input | The plant is enabled |
| `u1StaPro` | boolean input | A chiller staging change is in progress |
| `TSupSet` | real output, K | Chilled-water supply temperature setpoint |
| `dpSet` | real output, Pa | Chilled-water differential pressure setpoint |

## Parameters

| Name | Value | Meaning |
| --- | --- | --- |
| design supply temperature | 278.71 K | Used at `r` = 1; equals the BOPTEST baseline |
| warmest supply temperature | 282.15 K | Used at `r` ≤ 0.5 |
| design differential pressure | 239,125 Pa | Used at `r` ≥ 0.5; equals the BOPTEST baseline |
| lowest differential pressure | 95,650 Pa | Used at `r` = 0 |
| start delay | 900 s | Wait after the plant is enabled before any change |
| update period | 300 s | The reset value changes only at multiples of this period |
| ignored requests | 2 | Request counts up to this do not count |
| trim | −0.02 | Change per update when not responding |
| respond | +0.03 | Added per request above the ignored count |
| respond cap | +0.07 | Most the respond part may add in one update |
| staging hold | 900 s | Minimum time updates stay frozen after staging starts |

## Behaviour

1. While the plant is disabled, `r` = 1 and both setpoints are at design. Disabling takes effect
   at once.
2. After the plant is enabled, `r` stays at 1 for the start delay plus one update period (1,200 s
   with these values). The delay starts again at every re-enable.
3. Updates happen on the grid of multiples of the update period, counted from time 0, not from
   the enable. At each update the request count is read. If it exceeds the ignored count, the
   change is the trim plus the respond amount per extra request, with the respond part capped;
   otherwise the change is the trim. The new value is clamped to [0, 1].
4. When a staging change starts, updates freeze for at least the staging hold, and for as long
   as staging lasts if it lasts longer.
5. The supply temperature setpoint maps `r` linearly from the warmest value at `r` = 0.5 to the
   design value at `r` = 1, and stays at the warmest value below 0.5. The pressure setpoint maps
   `r` linearly from the lowest value at `r` = 0 to the design value at `r` = 0.5, and stays at
   design above 0.5. So trimming warms the water first and lowers the pressure second; responding
   restores the pressure first.

Edge instants, stated in the task because the prose above alone leaves them open: with an
enable at 0 s the first change lands at the 1,200 s update itself; requests that change at an
update instant count at that update; an update at the instant staging starts is already frozen
(staging from 3,000 s to 3,600 s freezes the 3,000, 3,300 and 3,600 s updates; 3,900 s applies);
the stored value only changes at updates, so the feedback path needs a sampled store.

## Deviations from the library block

- One pressure sensor (the library allows several, each with its own setpoint array).
- The library's sign-check assertions are omitted; they do not affect the outputs.
- Time is the engine's host tick (60 s steps in the vectors). The library is continuous: when an
  enable lands between update instants it shows the next update early. Our vectors enable on the
  update grid, where the two agree.

## Validation (done before any model sees this spec)

- `reference.py` is an independent model of the behaviour (stdlib Python, no engine code). Its
  self-check compares it with checkpoints worked out by hand; the same model, written for the
  G36 inventory, reproduced the library's own Dymola validation results with no mismatches. It
  writes `vectors.json`: 8 scenarios covering the start delay and trim cadence, ignore/respond/cap,
  the ceiling, saturation then release, short and long staging holds, disable and re-enable, and
  recovery from the floor.
- `graph.json` is a hand-built block graph (33 blocks). `crates/mm-bridge/tests/specs.rs` checks
  that it passes every scenario on the pinned engine, and that each of these mistakes fails at
  least one: start delay without the extra period, ignore count compared with "at least",
  uncapped respond, wrong respond amount, disabled output at 0, no staging hold, temperature and
  pressure halves swapped, no clamp at 100 %, wrong trim.
