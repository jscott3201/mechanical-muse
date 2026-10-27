"""Independent reference for the air handler's chilled-water requests, and the generator of
vectors.json.

Written from the behaviour of the Modelica Buildings Library's multizone AHU plant-request
sequence (chilled-water half) at commit a131864e, read as elementary block semantics: one-sided
hysteresis comparisons, true-delays that pass an input already true at the first evaluation,
and set/clear latches that act on rising edges. It does not use open-control-engine code or
engine output: the vectors it writes are the oracle the engine-run graph is checked against.

Frame semantics match the engine's host tick: one evaluation per tick, timers use elapsed model
time, "previous" means the previous tick.

Usage (stdlib only):
  python3 -I specs/ahu_chw_requests/reference.py          # self-check, then write vectors.json
  python3 -I specs/ahu_chw_requests/reference.py --check  # self-check only
"""
import json
import os
import sys

EPS = 1e-9
STEP = 60.0

P = dict(
    high3=3.0, high2=2.0, temp_band=0.1,  # K above setpoint, and the release band below that
    persist=120.0,                        # s a temperature condition must last
    valve_open=0.95, valve_reset=0.85, valve_closed=0.10,
    valve_band=0.05,
)


class Above:
    """True once u exceeds t; stays true until u is no longer above t - band."""

    def __init__(self, t, band):
        self.t, self.band, self.y = t, band, False

    def step(self, u):
        self.y = u > self.t - self.band if self.y else u > self.t
        return self.y


class Below:
    """True once u is under t; stays true until u is no longer under t + band."""

    def __init__(self, t, band):
        self.t, self.band, self.y = t, band, False

    def step(self, u):
        self.y = u < self.t + self.band if self.y else u < self.t
        return self.y


class Persist:
    """True once the input has been true for `d` seconds. An input already true at the first
    evaluation counts as having lasted (it passes straight through)."""

    def __init__(self, d):
        self.d, self.first, self.prev, self.since = d, True, False, None

    def step(self, t, u):
        if self.first:
            self.first, self.prev = False, u
            self.since = t - self.d if u else None
            return u
        if u and not self.prev:
            self.since = t
        self.prev = u
        return u and t - self.since >= self.d - EPS


class SetClear:
    """Starts as (set and not clear). Afterwards it changes only when set or clear turns on:
    then it becomes (set and not clear), using this tick's values."""

    def __init__(self):
        self.y, self.first, self.ps, self.pc = False, True, False, False

    def step(self, s, c):
        if self.first:
            self.first, self.y = False, s and not c
        elif (s and not self.ps) or (c and not self.pc):
            self.y = s and not c
        self.ps, self.pc = s, c
        return self.y


class Requests:
    def __init__(self, p=P):
        self.hi3 = Above(p["high3"], p["temp_band"])
        self.hi2 = Above(p["high2"], p["temp_band"])
        self.hi3_long = Persist(p["persist"])
        self.hi2_long = Persist(p["persist"])
        self.open = Above(p["valve_open"], p["valve_band"])
        self.reset_clear = Below(p["valve_reset"], p["valve_band"])
        self.plant_clear = Below(p["valve_closed"], p["valve_band"])
        self.reset_latch = SetClear()
        self.plant_latch = SetClear()

    def step(self, t, t_sup, t_set, valve):
        err = t_sup - t_set
        three = self.hi3_long.step(t, self.hi3.step(err))
        two = self.hi2_long.step(t, self.hi2.step(err))
        is_open = self.open.step(valve)
        one = self.reset_latch.step(is_open, self.reset_clear.step(valve))
        plant = self.plant_latch.step(is_open, self.plant_clear.step(valve))
        req = 3 if three else 2 if two else 1 if one else 0
        return req, 1 if plant else 0


def run(stim, horizon):
    model, out, k = Requests(), [], 0
    while k * STEP <= horizon + EPS:
        t = k * STEP
        vals = stim(t)
        out.append((t, vals, model.step(t, *vals)))
        k += 1
    return out


SET = 286.0  # supply air setpoint (K); temperatures are chosen so every error is exact in binary


def temp(err_by_time, valve_by_time=lambda t: 0.5):
    """Stimulus from piecewise error and valve functions."""
    return lambda t: (SET + err_by_time(t), SET, valve_by_time(t))


def steps(pairs, default):
    """Piecewise-constant function: value of the last (start, value) with start <= t."""
    def f(t):
        v = default
        for start, value in pairs:
            if t >= start:
                v = value
        return v
    return f


# Scenarios: (description, stimulus, horizon, checkpoints {t: (requests, plant)}) with the
# checkpoints worked out by hand from the behaviour, independent of the code above.
def scenarios():
    return {
        "steady_normal": (
            "Supply air 0.5 K above setpoint, valve half open: no requests.",
            temp(lambda t: 0.5), 1200, {0: (0, 0), 1200: (0, 0)}),
        "three_requests": (
            "Supply air 3.5 K above setpoint from 600 s: after 120 s the air handler sends 3 "
            "requests (the 2-request condition is also met, but 3 wins). When the error falls "
            "to 0.5 K at 1200 s the requests stop at once.",
            temp(steps([(600, 3.5), (1200, 0.5)], 0.5)), 1500,
            {540: (0, 0), 600: (0, 0), 660: (0, 0), 720: (3, 0), 1140: (3, 0), 1200: (0, 0)}),
        "two_requests": (
            "Supply air 2.5 K above setpoint from 600 s: 2 requests after 120 s.",
            temp(steps([(600, 2.5), (1200, 0.5)], 0.5)), 1500,
            {660: (0, 0), 720: (2, 0), 1140: (2, 0), 1200: (0, 0)}),
        "at_thresholds": (
            "The error is exactly 3 K from 600 s: that is not more than 3 K, so only the "
            "2-request condition starts. At 1200 s it is exactly 2 K, which is still inside the "
            "2-request release band (it releases only at 1.9 K or less), so 2 requests "
            "continue; at 1500 s it is 1.875 K and the requests stop.",
            temp(steps([(600, 3.0), (1200, 2.0), (1500, 1.875)], 0.5)), 1800,
            {660: (0, 0), 720: (2, 0), 1200: (2, 0), 1440: (2, 0), 1500: (0, 0)}),
        "three_release_band": (
            "3 requests from 420 s. At 900 s the error falls to 2.90625 K: still above 2.9 K, so "
            "3 requests continue. At 1200 s it is 2.890625 K: the 3-request condition releases "
            "and the 2-request condition, met all along, gives 2 at once. At 1500 s, 1.875 K: 0.",
            temp(steps([(300, 3.5), (900, 2.90625), (1200, 2.890625), (1500, 1.875)], 0.5)),
            1800, {360: (0, 0), 420: (3, 0), 900: (3, 0), 1140: (3, 0), 1200: (2, 0),
                   1500: (0, 0)}),
        "two_from_below": (
            "The error rises to exactly 2 K at 300 s: not above 2 K, so nothing starts. At 900 s "
            "it is 2.0625 K and 2 requests follow 120 s later. At 1500 s it eases to 1.90625 K, "
            "still above 1.9 K, so they continue; at 1800 s, 1.890625 K, they stop.",
            temp(steps([(300, 2.0), (900, 2.0625), (1500, 1.90625), (1800, 1.890625)], 0.5)),
            2100, {300: (0, 0), 840: (0, 0), 900: (0, 0), 960: (0, 0), 1020: (2, 0),
                   1500: (2, 0), 1740: (2, 0), 1800: (0, 0)}),
        "starts_two": (
            "The error is 2.5 K from the first frame: 2 requests at 0 s, as a condition already "
            "met at the start counts as having lasted.",
            temp(steps([(600, 0.5)], 2.5)), 900, {0: (2, 0), 540: (2, 0), 600: (0, 0)}),
        "two_beats_valve": (
            "Valve latched from the start (1 request); the error rises to 2.5 K at 600 s, giving "
            "2 requests from 720 s; when it clears at 1200 s the latched 1 request remains.",
            temp(steps([(600, 2.5), (1200, 0.5)], 0.5), lambda t: 0.97), 1500,
            {0: (1, 1), 660: (1, 1), 720: (2, 1), 1140: (2, 1), 1200: (1, 1)}),
        "timer_restarts": (
            "The error jumps to 3.5 K at 600 s, drops to 1 K at 660 s and returns to 3.5 K at "
            "720 s. The 120 s count starts again at 720 s, so requests begin at 840 s.",
            temp(steps([(600, 3.5), (660, 1.0), (720, 3.5)], 0.5)), 1200,
            {600: (0, 0), 660: (0, 0), 720: (0, 0), 780: (0, 0), 840: (3, 0)}),
        "starts_high": (
            "The error is 3.5 K from the first frame: a condition already true at the start "
            "counts as having lasted, so 3 requests are sent at 0 s. After it clears at 600 s, "
            "a new rise at 900 s waits the full 120 s again.",
            temp(steps([(600, 0.5), (900, 3.5)], 3.5)), 1200,
            {0: (3, 0), 540: (3, 0), 600: (0, 0), 900: (0, 0), 960: (0, 0), 1020: (3, 0)}),
        "valve_latch": (
            "The cooling valve opens past 95 % at 300 s: 1 request and a plant request at once. "
            "It stays latched when the valve eases to 92 % and 88 %, and through a second "
            "opening to 97 %. At 1500 s the valve is 84 %, under 85 %: the request clears but "
            "the plant request stays. At 1800 s, 93 % is not enough to latch again; at 2100 s, "
            "96 % is.",
            temp(lambda t: 0.5, steps([(300, 0.96), (600, 0.92), (900, 0.88), (1200, 0.97),
                                       (1500, 0.84), (1800, 0.93), (2100, 0.96)], 0.5)), 2400,
            {240: (0, 0), 300: (1, 1), 600: (1, 1), 900: (1, 1), 1200: (1, 1), 1500: (0, 1),
             1800: (0, 1), 2100: (1, 1)}),
        "valve_boundaries": (
            "A valve at exactly 95 % does not latch; 95.1 % does. At exactly 85 % the request "
            "stays; at 84.9 % it clears.",
            temp(lambda t: 0.5, steps([(300, 0.95), (600, 0.951), (900, 0.85), (1200, 0.849)],
                                      0.5)), 1500,
            {300: (0, 0), 600: (1, 1), 900: (1, 1), 1200: (0, 1)}),
        "plant_request": (
            "The valve starts at 97 %, so both are latched from the first frame. Falling to "
            "50 % clears the reset request but not the plant request; that needs the valve "
            "under 10 %: exactly 10 % keeps it, 9 % clears it. Reopening past 95 % sets both.",
            temp(lambda t: 0.5, steps([(600, 0.5), (900, 0.10), (1200, 0.09), (1500, 0.5),
                                       (1800, 0.97)], 0.97)), 2100,
            {0: (1, 1), 540: (1, 1), 600: (0, 1), 900: (0, 1), 1200: (0, 0), 1500: (0, 0),
             1800: (1, 1)}),
        "clear_and_set_together": (
            "Latched from the start; the valve drops to 80 % at 300 s (cleared) and jumps "
            "straight to 97 % at 600 s. In that frame the clear condition ends as the open "
            "condition starts, and the latch sets again.",
            temp(lambda t: 0.5, steps([(300, 0.80), (600, 0.97)], 0.97)), 900,
            {0: (1, 1), 300: (0, 1), 600: (1, 1), 900: (1, 1)}),
        "temperature_beats_valve": (
            "Valve latched from the start (1 request); the error rises to 3.5 K at 600 s, giving "
            "3 requests from 720 s; when it clears at 1200 s the latched 1 request remains.",
            temp(steps([(600, 3.5), (1200, 0.5)], 0.5), lambda t: 0.97), 1500,
            {0: (1, 1), 660: (1, 1), 720: (3, 1), 1200: (1, 1), 1500: (1, 1)}),
    }


def self_check():
    ok = True
    for name, (_, stim, horizon, checks) in scenarios().items():
        trace = {t: out for t, _, out in run(stim, horizon)}
        for t, want in checks.items():
            if trace[float(t)] != want:
                ok = False
                print(f"FAIL {name} at t={t}: {trace[float(t)]} want {want}")
    print("self-check", "PASS" if ok else "FAIL")
    return ok


def windows(trace, index):
    """Compress a per-tick integer output into inclusive windows of equal value."""
    out, start, value, last = [], None, None, None
    for t, _, outputs in trace:
        v = outputs[index]
        if start is None or v != value:
            if start is not None:
                out.append((start, last, value))
            start, value = t, v
        last = t
    out.append((start, last, value))
    return out


def vectors():
    files = []
    for name, (description, stim, horizon, _) in scenarios().items():
        trace = run(stim, horizon)
        inputs = {}
        for key, i in (("TAirSup", 0), ("TAirSupSet", 1), ("uCooCoi", 2)):
            seq, prev = [], None
            for t, vals, _ in trace:
                if vals[i] != prev:
                    seq.append({"t": t, "value": vals[i]})
                    prev = vals[i]
            inputs[key] = seq[0]["value"] if len(seq) == 1 else seq
        expect = []
        for output, index in (("yChiWatResReq", 0), ("yChiPlaReq", 1)):
            for a, b, v in windows(trace, index):
                expect.append({"output": output, "from_s": a, "to_s": b, "equals": v})
        files.append({"name": name, "description": description, "inputs": inputs,
                      "expect": expect})
    return {"schema": "mechanical-muse/vectors/v1",
            "clock": {"step_s": STEP, "horizon_s": max(s[2] for s in scenarios().values())},
            "scenarios": files}


if __name__ == "__main__":
    if not self_check():
        sys.exit(1)
    if "--check" in sys.argv:
        sys.exit(0)
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "vectors.json")
    with open(path, "w") as f:
        json.dump(vectors(), f, indent=1)
        f.write("\n")
    print("wrote", path)
