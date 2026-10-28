"""Independent reference for the home price-response setpoint, and the generator of vectors.json.

Written from spec.md alone (stdlib Python, no engine code, no engine output): the vectors it
writes are the oracle the engine-run graph is checked against. The sequence keeps no state, so
each frame's setpoint depends only on that frame's inputs.

Usage (stdlib only):
  python3 -I specs/home_price_response/reference.py          # self-check, write vectors
  python3 -I specs/home_price_response/reference.py --check  # self-check only
"""
import json
import os
import sys

STEP = 60.0
HORIZON = 240
TOL = 1e-6

# Kelvin and price per kWh. See spec.md "Parameters".
P = dict(
    margin=0.2,          # K above the comfort floor in normal operation
    away=293.65,         # K, lowest normal setpoint (20.5 degC) when the floor is low
    peak_share=0.75,     # a price is peak when at least this share of the way from min to max
    min_spread=0.01,     # price per kWh; a flatter day has no peak hours
    preheat=1.0,         # K added before peak hours
    ceiling_gap=2.0,     # K the preheat stays below the coming ceiling
    coast_drop=1.5,      # K below the base setpoint while coasting
    coast_margin=0.1,    # K above the coming floor while coasting
    warm_band=0.5,       # K above normal at which the room counts as already warm
)

PRICES = [f"pri{h:02d}" for h in range(24)]
FLOORS = ["TLo00", "TLo01", "TLo02"]
CEILINGS = ["THi00", "THi01", "THi02", "THi03"]

OCC_LO, OCC_HI = 294.15, 297.15      # occupied comfort band (21 to 24 degC)
AWAY_LO, AWAY_HI = 288.15, 303.15    # unoccupied band (15 to 30 degC)


def setpoint(x):
    """The spec's rule for one frame. `x` maps every input name to its value."""
    prices = [x[n] for n in PRICES]
    hi, lo = max(prices), min(prices)
    spread = hi - lo
    threshold = lo + P["peak_share"] * spread
    has_peak = spread > P["min_spread"]
    peak_now = has_peak and x["pri00"] >= threshold
    peak_soon = has_peak and max(x["pri01"], x["pri02"], x["pri03"]) >= threshold

    floor_soon = max(x[n] for n in FLOORS)
    ceiling_soon = min(x[n] for n in CEILINGS)
    base = max(x["TLo00"] + P["margin"], P["away"])
    normal = max(base, floor_soon + P["margin"])

    if peak_now:
        return max(base - P["coast_drop"], floor_soon + P["coast_margin"])
    warm = x["TZon"] > normal + P["warm_band"]
    if peak_soon and not warm:
        return max(min(normal + P["preheat"], ceiling_soon - P["ceiling_gap"]), normal)
    return normal


def frame(prices=None, floors=(OCC_LO,) * 3, ceilings=(OCC_HI,) * 4, tzon=294.35):
    """One frame of inputs. Prices default to a flat 0.25 per kWh."""
    prices = list(prices) if prices is not None else [0.25] * 24
    x = {n: v for n, v in zip(PRICES, prices)}
    x.update({n: v for n, v in zip(FLOORS, floors)})
    x.update({n: v for n, v in zip(CEILINGS, ceilings)})
    x["TZon"] = tzon
    return x


def day(peak_at=(), low=0.125, high=0.25, mid=None):
    """24 hourly prices: `high` at the hours in `peak_at`, `low` elsewhere (optional `mid` hours).
    Values are exact in binary so threshold comparisons are exact: with 0.125 and 0.25 the
    threshold is 0.21875."""
    p = [low] * 24
    for h, v in (mid or {}).items():
        p[h] = v
    for h in peak_at:
        p[h] = high
    return p


# Scenarios: (name, description, frame, or list of (t, frame) for frames that change)
SCENARIOS = [
    ("flat_prices", "Every hour costs the same: no peak hours, so the setpoint is the comfort "
     "floor plus 0.2 K (occupied: 294.35 K).", frame()),
    ("small_spread", "Prices differ by only 0.0078125 per kWh across the day, under the 0.01 "
     "minimum, so nothing counts as peak even though this hour is the most expensive.",
     frame(day(peak_at=[0, 1, 2], low=0.25, high=0.2578125))),
    ("peak_now_occupied", "This hour is a peak hour while occupied: coast to 0.1 K above the "
     "coming floor (294.25 K), since 1.5 K below the base would be under the floor.",
     frame(day(peak_at=[0, 1, 2, 3]))),
    ("peak_now_away", "This hour is a peak hour while away (floor 15 degC): coast 1.5 K below the "
     "20.5 degC away setpoint, 292.15 K.",
     frame(day(peak_at=[0, 1]), floors=(AWAY_LO,) * 3, ceilings=(AWAY_HI,) * 4, tzon=293.65)),
    ("peak_soon_occupied", "A peak hour starts in 2 hours while occupied: preheat by 1 K, capped "
     "2 K below the 24 degC ceiling: min(295.35, 295.15) = 295.15 K.",
     frame(day(peak_at=[2, 3, 4]))),
    ("peak_soon_away", "A peak hour starts in 3 hours while away: preheat to the away setpoint "
     "plus 1 K, 294.65 K (the 30 degC ceiling is far).",
     frame(day(peak_at=[3, 4]), floors=(AWAY_LO,) * 3, ceilings=(AWAY_HI,) * 4, tzon=293.0)),
    ("peak_in_four_hours", "The nearest peak hour is 4 hours out, outside the 3-hour look-ahead: "
     "no preheat yet, 294.35 K.", frame(day(peak_at=[4, 5]))),
    ("peak_soon_but_warm", "A peak hour is coming but the room is already more than 0.5 K above "
     "normal (294.9 K > 294.85 K): no preheat, 294.35 K.",
     frame(day(peak_at=[1, 2]), tzon=294.9)),
    ("peak_soon_warm_edge", "The room is exactly 0.5 K above normal (about 294.85 K, the same "
     "sum the sequence computes), which is not above it: preheat to 295.15 K.",
     frame(day(peak_at=[1, 2]), tzon=(OCC_LO + P["margin"]) + P["warm_band"])),
    ("ceiling_drops_soon", "A peak hour is coming and the ceiling falls to 23 degC 3 hours ahead: "
     "the preheat cap (21 degC) is below normal, so the setpoint stays at normal, 294.35 K.",
     frame(day(peak_at=[2]), ceilings=(OCC_HI, OCC_HI, OCC_HI, 296.15))),
    ("arrival_in_two_hours", "Away now (floor 15 degC) but occupied from 2 hours ahead: normal "
     "rises early to the coming floor plus 0.2 K, 294.35 K, so the slab is warm on arrival.",
     frame(floors=(AWAY_LO, AWAY_LO, OCC_LO), ceilings=(AWAY_HI,) * 4, tzon=293.65)),
    ("arrival_during_peak", "Away, a peak hour now, and occupied from 1 hour ahead: coasting "
     "never goes below the coming floor plus 0.1 K, 294.25 K.",
     frame(day(peak_at=[0]), floors=(AWAY_LO, OCC_LO, OCC_LO), ceilings=(AWAY_HI,) * 4)),
    ("arrival_before_peak", "Away now, home from 1 hour ahead, a peak hour in 2 hours: normal is "
     "the coming floor plus 0.2 K (294.35 K), and the room at 294.5 K is not above normal plus "
     "0.5 K, so preheat to 295.15 K (from normal, not from the 20.5 degC away base).",
     frame(day(peak_at=[2, 3]), floors=(AWAY_LO, OCC_LO, OCC_LO),
           ceilings=(AWAY_HI, OCC_HI, OCC_HI, OCC_HI), tzon=294.5)),
    ("departure_during_peak", "Home now, away from 1 hour ahead, a peak hour now: the floor now "
     "still counts, so coasting stops at 21 degC plus 0.1 K, 294.25 K.",
     frame(day(peak_at=[0]), floors=(OCC_LO, AWAY_LO, AWAY_LO),
           ceilings=(OCC_HI, AWAY_HI, AWAY_HI, AWAY_HI))),
    ("short_visit_during_peak", "Away now and 2 hours ahead, home only for the hour starting in 1 "
     "hour, a peak hour now: coast no lower than that floor plus 0.1 K, 294.25 K.",
     frame(day(peak_at=[0]), floors=(AWAY_LO, OCC_LO, AWAY_LO), ceilings=(AWAY_HI,) * 4)),
    ("ceiling_lower_now", "A peak hour is coming and the ceiling is 23 degC now: preheat capped "
     "below normal, so normal, 294.35 K.",
     frame(day(peak_at=[2]), ceilings=(296.15, OCC_HI, OCC_HI, OCC_HI))),
    ("ceiling_lower_in_one_hour", "A peak hour is coming and the ceiling is 23 degC in 1 hour: "
     "normal, 294.35 K.", frame(day(peak_at=[2]), ceilings=(OCC_HI, 296.15, OCC_HI, OCC_HI))),
    ("threshold_edge", "This hour's price equals the threshold exactly (0.21875 with a 0.125 to "
     "0.25 day): at the threshold counts as peak, so coast to 294.25 K.",
     frame(day(peak_at=[12], mid={0: 0.21875}))),
    ("just_below_threshold", "This hour's price is just under the threshold (0.21875 - 2^-10) and "
     "the next three hours are cheap: normal, 294.35 K.",
     frame(day(peak_at=[12], mid={0: 0.21875 - 2.0 ** -10}))),
    ("max_in_last_hour", "The day's most expensive hour is the last one (hour 23); this hour sits "
     "below the threshold that hour sets: normal. Every hourly price must reach the max.",
     frame(day(peak_at=[23], mid={0: 0.1875}))),
    ("min_in_last_hour", "The cheapest hour is the last one (0.0 at hour 23); a 0.1875 hour now is "
     "then a peak (threshold 0.1875): coast. Every hourly price must reach the min.",
     frame(day(peak_at=[], low=0.25, mid={0: 0.1875, 23: 0.0}))),
    ("rolling_day", "The forecast rolls forward: normal, then preheat when the peak comes within "
     "3 hours, then coast when it arrives, then normal once it has passed.",
     [(0.0, frame(day(peak_at=[5, 6]))),
      (60.0, frame(day(peak_at=[3, 4]))),
      (120.0, frame(day(peak_at=[0, 1]))),
      (180.0, frame(day(peak_at=[22, 23])))]),
]


def expectations(name, frames):
    """Windowed expectations: one per frame change, covering the ticks until the next change."""
    out = []
    for i, (t, x) in enumerate(frames):
        end = frames[i + 1][0] - STEP if i + 1 < len(frames) else float(HORIZON)
        out.append({"output": "TSet", "from_s": t, "to_s": end,
                    "equals": round(setpoint(x), 9), "tolerance": TOL})
    return out


def inputs(frames):
    names = PRICES + FLOORS + CEILINGS + ["TZon"]
    result = {}
    for n in names:
        values = [(t, x[n]) for t, x in frames]
        if all(v == values[0][1] for _, v in values):
            result[n] = values[0][1]
        else:
            result[n] = [{"t": t, "value": v} for t, v in values]
    return result


def build():
    scenarios = []
    for name, description, spec in SCENARIOS:
        frames = spec if isinstance(spec, list) else [(0.0, spec)]
        scenarios.append({"name": name, "description": description,
                          "inputs": inputs(frames), "expect": expectations(name, frames)})
    return {"schema": "mechanical-muse/vectors/v1",
            "clock": {"step_s": STEP, "horizon_s": HORIZON}, "scenarios": scenarios}


def self_check():
    """Checkpoints worked out by hand from spec.md."""
    hand = {
        "flat_prices": 294.35, "small_spread": 294.35, "peak_now_occupied": 294.25,
        "peak_now_away": 292.15, "peak_soon_occupied": 295.15, "peak_soon_away": 294.65,
        "peak_in_four_hours": 294.35, "peak_soon_but_warm": 294.35,
        "peak_soon_warm_edge": 295.15, "ceiling_drops_soon": 294.35,
        "arrival_in_two_hours": 294.35, "arrival_during_peak": 294.25,
        "arrival_before_peak": 295.15, "departure_during_peak": 294.25,
        "short_visit_during_peak": 294.25, "ceiling_lower_now": 294.35,
        "ceiling_lower_in_one_hour": 294.35, "threshold_edge": 294.25, "just_below_threshold": 294.35, "max_in_last_hour": 294.35,
        "min_in_last_hour": 294.25,
    }
    rolling = [294.35, 295.15, 294.25, 294.35]
    errors = []
    for name, _, spec in SCENARIOS:
        if isinstance(spec, list):
            got = [setpoint(x) for _, x in spec]
            if any(abs(a - b) > 1e-9 for a, b in zip(got, rolling)):
                errors.append(f"{name}: {got} != {rolling}")
        elif abs(setpoint(spec) - hand[name]) > 1e-9:
            errors.append(f"{name}: {setpoint(spec)} != {hand[name]}")
    if errors:
        sys.exit("self-check failed:\n" + "\n".join(errors))
    print(f"self-check ok: {len(SCENARIOS)} scenarios")


if __name__ == "__main__":
    self_check()
    if "--check" not in sys.argv:
        path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "vectors.json")
        with open(path, "w") as f:
            json.dump(build(), f, indent=1)
            f.write("\n")
        print("wrote", path)
