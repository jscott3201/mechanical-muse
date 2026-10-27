"""Independent reference for one air handler's duct static pressure reset, and the generator of
vectors.json.

Written from the behaviour of the Modelica Buildings Library's generic trim-and-respond block as
the multizone AHU supply fan sequence uses it (commit a131864e), read as block semantics, with
the parameter values in spec.md. It does not use open-control-engine code or engine output: the
vectors it writes are the oracle the engine-run graph is checked against.

Frame semantics match the engine's host tick: one evaluation per tick; sampled blocks fire on the
first tick at or after k * sample period (grid anchored at t = 0); timers use elapsed model time.

Usage (stdlib only):
  python3 -I specs/ahu_static_pressure_reset/reference.py          # self-check, write vectors
  python3 -I specs/ahu_static_pressure_reset/reference.py --check  # self-check only
"""
import json
import os
import sys

EPS = 1e-9
STEP = 60.0

# Pa. The initial and highest setpoint equals BOPTEST's own constant 400 Pa; the lowest is
# BOPTEST's lower bound for the setpoint.
P = dict(
    init=400.0, lo=50.0, hi=400.0,
    dtDel=600.0, dtRes=120.0,
    ign=2, tri=-12.0, rsp=15.0, rsp_max=32.0,
)


class TrueDelay:
    """Output true once the input has been true for `delay` seconds (an input true at the first
    evaluation still has to last the full delay)."""

    def __init__(self, delay):
        self.d, self.first, self.u_prev, self.t_rise = delay, True, False, None

    def step(self, t, u):
        if self.first or (u and not self.u_prev):
            self.t_rise = t
        self.first, self.u_prev = False, u
        return u and t - self.t_rise >= self.d - EPS


class Clock:
    """Fires on the first tick at or after each multiple of `period`."""

    def __init__(self, period):
        self.p, self.next = period, 0.0

    def fire(self, t):
        if t + EPS < self.next:
            return False
        while self.next <= t + EPS:
            self.next += self.p
        return True


class StaticPressureReset:
    def __init__(self, p=P):
        self.p = p
        self.delay = TrueDelay(p["dtDel"] + p["dtRes"])
        self.clock = Clock(p["dtRes"])
        self.requests = 0.0      # requests as sampled at the last update
        self.prev = p["init"]    # setpoint stored at the update before the last
        self.stored = p["init"]  # setpoint stored at the last update

    def step(self, t, n_req, fan_on):
        p = self.p
        active = self.delay.step(t, fan_on)
        sample = self.clock.fire(t)
        if sample:
            self.requests = float(n_req)
            self.prev = self.stored
        excess = self.requests - p["ign"]
        if active and excess > 0:
            change = p["tri"] + min(excess * p["rsp"], p["rsp_max"])
        else:
            change = p["tri"] if active else 0.0
        value = max(min(self.prev + change, p["hi"]), p["lo"])
        if not fan_on:
            value = p["init"]
        if sample:
            self.stored = value
        return value if active else p["init"]


def run(stim, horizon):
    model, out, k = StaticPressureReset(), [], 0
    while k * STEP <= horizon + EPS:
        t = k * STEP
        n_req, fan_on = stim(t)
        out.append((t, (n_req, fan_on), model.step(t, n_req, fan_on)))
        k += 1
    return out


# Scenarios: (description, stimulus, horizon, checkpoints {t: dpSet in Pa}) with checkpoints
# worked out by hand from the behaviour, independent of the code above. Updates fall on
# multiples of 120 s; with the fan on from 0 s the first change is at the 720 s update.
def scenarios():
    def stepped(t):
        for start, n in ((2520, 10), (2280, 5), (2040, 4), (1800, 3), (1560, 2)):
            if t >= start:
                return n
        return 0

    return {
        "trim_from_fan_on": (
            "Fan on from the start, no requests. The setpoint holds at 400 Pa for the start "
            "delay plus one update period (720 s), then trims 12 Pa every 120 s down to its "
            "50 Pa floor.",
            lambda t: (0, True), 5400,
            {0: 400.0, 660: 400.0, 720: 388.0, 780: 388.0, 840: 376.0, 1440: 316.0,
             4080: 52.0, 4140: 52.0, 4200: 50.0, 5400: 50.0}),
        "ignore_respond_cap": (
            "After trimming to 316 Pa, requests step 2, 3, 4, 5, 10 on update instants. Up to 2 "
            "requests are ignored; above that each extra request adds 15 Pa before the 12 Pa "
            "trim, capped at 32 Pa, and the setpoint never rises above 400 Pa.",
            lambda t: (stepped(t), True), 3000,
            {1500: 316.0, 1560: 304.0, 1680: 292.0, 1800: 295.0, 1920: 298.0, 2040: 316.0,
             2160: 334.0, 2280: 354.0, 2400: 374.0, 2520: 394.0, 2640: 400.0, 3000: 400.0}),
        "ceiling_clamp": (
            "Three requests from the start: the net change is +3 Pa, but the setpoint cannot "
            "rise above 400 Pa.",
            lambda t: (3, True), 3000, {720: 400.0, 3000: 400.0}),
        "saturate_then_release": (
            "Five requests until 3000 s hold the setpoint at its 400 Pa ceiling; when they stop, "
            "trimming starts from 400 Pa at the next update, not from some value above it.",
            lambda t: (5 if t < 3000 else 0, True), 3600,
            {2940: 400.0, 3000: 388.0, 3120: 376.0, 3600: 328.0}),
        "off_grid_requests": (
            "Requests that change between update instants are counted at the next update: four "
            "requests from 1500 s count at 1560 s and stop counting at 2040 s after they end at "
            "1950 s; a pulse of six requests from 2100 s to 2160 s ends before an update reads "
            "it and is never counted.",
            lambda t: (4 if 1500 <= t < 1950 else 6 if 2100 <= t < 2160 else 0, True), 2400,
            {1440: 316.0, 1500: 316.0, 1560: 334.0, 1680: 352.0, 1800: 370.0, 1920: 388.0,
             2040: 376.0, 2160: 364.0, 2280: 352.0, 2400: 340.0}),
        "fan_off_and_on": (
            "The fan stops from 3000 s to 3600 s: the setpoint returns to 400 Pa at once, and "
            "the start delay begins again when the fan restarts.",
            lambda t: (0, not (3000 <= t < 3600)), 4500,
            {2940: 172.0, 3000: 400.0, 3540: 400.0, 4260: 400.0, 4320: 388.0, 4440: 376.0}),
    }


def self_check():
    ok = True
    for name, (_, stim, horizon, checks) in scenarios().items():
        trace = {t: y for t, _, y in run(stim, horizon)}
        for t, want in checks.items():
            if abs(trace[float(t)] - want) > 1e-9:
                ok = False
                print(f"FAIL {name} at t={t}: dpSet={trace[float(t)]} want {want}")
    print("self-check", "PASS" if ok else "FAIL")
    return ok


def windows(trace, tol):
    """Compress a per-tick output into inclusive windows of equal value."""
    out, start, value, last = [], None, None, None
    for t, _, v in trace:
        if start is None or abs(v - value) > tol:
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
        for key, i, cast in (("nReqPre", 0, int), ("u1FanOn", 1, bool)):
            steps, prev = [], object()
            for t, stim_vals, _ in trace:
                v = cast(stim_vals[i])
                if v != prev:
                    steps.append({"t": t, "value": v})
                    prev = v
            inputs[key] = steps[0]["value"] if len(steps) == 1 else steps
        expect = [{"output": "dpSet", "from_s": a, "to_s": b, "equals": round(v, 9),
                   "tolerance": 1e-6} for a, b, v in windows(trace, 1e-9)]
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
