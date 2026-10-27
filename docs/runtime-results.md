# Runtime results

Live runs of verified sequences against BOPTEST. Each entry states the setup, what was seen, and
what it does not show. Run records (JSON) stay local in the gitignored `runs/`; the numbers
below are copied from them.

## 2026-10-26: chilled-water plant reset, first live run

Setup: BOPTEST v0.9.0 (`compose.yaml`, local Docker on Apple silicon, x86 worker emulated),
`multizone_office_complex_air`. Start at 05:30 on day 185 (a Monday) after a 1-day warm-up
under baseline control; 120 steps of 120 s (4 simulated hours). The sequence is the
`specs/chw_plant_reset` reference graph, loaded only after its 10 vector scenarios replayed
green (content id checked at load). Bindings: request count constant 0 (the AHU request
sequence is not wired yet), plant enable = any floor's AHU occupancy flag, staging constant
false. Outputs clamped to 278.15..288.15 K and 0..478,250 Pa, fail-safe policy release.
Command: `MM_LIVE_STEPS=120 cargo test -p mm-runtime --test live_boptest -- --ignored`.

| Time | TSupSet (K) | Measured CHW supply (K) | dpSet (Pa) | Measured dp (Pa) | Chiller power (kW) | CHW pump power (kW) |
| --- | --- | --- | --- | --- | --- | --- |
| 05:30 | 278.71 | 282.28 | 239,125 | 67,185 | 1,082 | 300 |
| 05:54 | 278.85 | 282.18 | 239,125 | 267,017 | 1,559 | 448 |
| 06:30 | 279.95 | 280.81 | 239,125 | 246,229 | 1,043 | 270 |
| 07:06 | 280.91 | 280.60 | 239,125 | 243,486 | 617 | 255 |
| 07:54 | 282.15 | 281.17 | 239,125 | 238,877 | 433 | 255 |
| 08:30 | 282.15 | 281.69 | 193,213 | 201,237 | 439 | 248 |
| 09:28 | 282.15 | 282.03 | 130,084 | 136,239 | 462 | 238 |

What it shows:

- The loop ran all 120 steps with no fail-safe event (no stale data, refused frame, rejected or
  held value). Every tick wrote both setpoints with `_activate` = 1.
- The sequence behaved as its spec says on the live clock: setpoints held at design for the
  start delay plus one update period; the first trim (278.8476 K) is the same value the offline
  test and the spec vectors give; the supply temperature reached its warm limit after 25
  updates, and only then did the pressure setpoint start to fall.
- BOPTEST followed the writes: measured supply temperature and loop pressure track the
  setpoints with a lag of minutes.
- The floor AHU occupancy flag was already 1 at 05:30 (the case documentation gives 06:00 as
  the start of occupied hours; the AHUs start early). Day 184, the case's peak cooling day, is
  a Sunday and stays unoccupied in the morning.

What it does not show:

- No energy or comfort claim. Chiller and pump power fell over the run, but this window is
  also the morning pull-down, and there is no baseline-only run of the same window yet. The
  KPI comparison needs that run (point map section 10, item 7).
- With zero requests the reset can only trim. Responding to requests needs the AHU request
  sequence wired as an upstream sequence.

Speed: initialize took 939 to 1,114 s with a 1-day warm-up. Steps on this summer morning took
0.84 s each (101 s for 120 steps), against 9.4 s each in a January spike: step cost depends
strongly on the period simulated, so time a demo window before relying on it.
