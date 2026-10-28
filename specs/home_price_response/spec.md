# Home price response

A heating setpoint for one home with an air-to-water heat pump and floor heating. It moves heat
out of the hours when electricity costs the most, and keeps the room inside its comfort band.
Demand response programs add up many homes like this one; the grid sees the sum. BOPTEST's own
controller holds the setpoint just above the comfort floor whatever the price.

Our own design, written for BOPTEST `bestest_hydronic_heat_pump` (a simulated home in Brussels,
Belgium, heating only, prices in euros) and tuned in a headroom probe on that building
(`docs/results-log.md`, 2026-10-28). It is not taken from any standard or library sequence.

The sequence keeps no state: each step, the setpoint depends only on that step's inputs. Looking
ahead is the runtime's job only in the plainest sense: it passes BOPTEST's forecast values as
inputs (24 hourly prices, the comfort limits for the next hours). Every decision about them is
made inside the verified graph.

## Interface

| Point | Type | Meaning |
| --- | --- | --- |
| `pri00`..`pri23` | real inputs, per kWh | Electricity price for the current hour (`pri00`) and the next 23 hours |
| `TLo00`..`TLo02` | real inputs, K | Lower comfort limit (floor) now, in 1 h and in 2 h |
| `THi00`..`THi03` | real inputs, K | Upper comfort limit (ceiling) now and in 1, 2 and 3 h |
| `TZon` | real input, K | Room temperature now |
| `TSet` | real output, K | Heating setpoint |

## Parameters

| Name | Value | Meaning |
| --- | --- | --- |
| peak share | 0.75 | An hour is peak-price when its price is at least this share of the way from the day's lowest to highest price |
| minimum spread | 0.01 per kWh | A day whose prices differ by no more than this has no peak-price hours |
| margin | 0.2 K | Normal setpoint above the comfort floor (BOPTEST's own controller uses the same) |
| away setpoint | 20.5 °C (293.65 K) | Lowest normal setpoint, used while nobody is home and the floor is 15 °C |
| coast drop | 1.5 K | How far below base the setpoint drops in a peak-price hour |
| coast margin | 0.1 K | Coasting never goes below the coming floor plus this |
| preheat | 1.0 K | Added to normal before a peak-price hour |
| ceiling gap | 2.0 K | Preheat stays this far below the coming ceiling (the slab and the sun keep heating) |
| warm band | 0.5 K | No preheat when the room is already more than this above normal |

## Behavior

1. **Peak-price hours.** Over the 24 hourly prices, threshold = lowest + 0.75 × (highest −
   lowest). If the prices differ by more than 0.01, the current hour is peak-price when its price
   is at or above the threshold, and a peak-price hour is coming when any of the next three
   hours is at or above it.
2. **Normal.** base = the larger of (floor now + 0.2 K) and 20.5 °C. normal = the larger of base
   and (the highest floor from now through the next 2 h + 0.2 K), so the slab is already warm
   when people come home.
3. **During a peak-price hour: coast.** Setpoint = the larger of (base − 1.5 K) and (the highest
   floor from now through the next 2 h + 0.1 K).
4. **Before a peak-price hour: preheat.** Unless the room is already more than 0.5 K above
   normal, setpoint = normal + 1 K, capped at the lowest ceiling from now through the next 3 h
   minus 2 K, and never below normal.
5. **Otherwise** the setpoint is normal.

A peak-price hour now takes precedence over one coming.

## Validation (done before any model sees this spec)

- `reference.py` is an independent model of the behavior (stdlib Python, no engine code). Its
  self-check compares it with checkpoints worked out by hand, and it writes `vectors.json`: 22
  scenarios covering flat and nearly flat days, peak now and coming while occupied and away, a
  peak just beyond the look-ahead, an already-warm room (and the exact edge), ceilings that drop
  now, in 1 h and in 3 h, an arrival before and during a peak, a departure and a one-hour visit
  during a peak, the threshold edge and just below it, the day's extreme price in the last hour,
  and a forecast that rolls through a peak.
- `graph.json` is a hand-built block graph (33 blocks; `Reals.MultiMax` and `Reals.MultiMin`
  take the max and min of the hourly values). `crates/mm-bridge/tests/specs_home_price.rs`
  checks that it passes every scenario on the pinned engine, and that each of 21 mistakes fails
  at least one: a different peak share, a strict threshold, no or a zero minimum spread, a day
  max or min that misses the last hour, a four-hour look-ahead, ignoring the coming floor or
  ceiling, a different margin, away setpoint, coast drop, coast margin, preheat, ceiling gap or
  warm band, preheating from base or judging warmth against base instead of normal, a coming
  floor that misses the floor now, preheating a warm room, and preheat taking precedence over
  coasting.

## Caveats

- Tuned on BOPTEST's two heating windows for this building with a perfect price forecast. On two
  windows it was not tuned on (14 days from day 40 and from day 315) the rule, run from
  `reference.py`, still beat BOPTEST's controller on cost (−6.9 %, −6.0 %) and discomfort
  (−45 %, −72 %) and cut mean power in the most expensive quarter of 15-minute prices by 37 % and 23 % (`docs/results-log.md`).
  Through the runtime, on BOPTEST's two heating windows and on these two, the verified graph sent
  exactly the setpoints `reference.py` computes (`docs/runtime-results.md`).
- Moving heat raises the single highest 15-minute draw (BOPTEST's peak demand KPI rose 16 % and
  4 % in the two live windows). Nothing here staggers that rebound across homes.
- Prices in this building swing little (about 0.17 to 0.26 euro per kWh including fixed fees),
  so the per-home saving is small; the point is the power moved out of peak-price hours.
