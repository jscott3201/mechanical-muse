"""Independent reference for one zone's static pressure reset requests, and the generator of
vectors.json.

Written from the behaviour of the Modelica Buildings Library's VAV terminal system-request
sequence, static pressure half (commit a131864e, as vendored by the pinned open-control-engine),
read as elementary block semantics: one-sided hysteresis comparisons and a true-delay. It does not
use open-control-engine code or engine output: the vectors it writes are the oracle the
engine-run graph is checked against.

Frame semantics match the engine's host tick: one evaluation per tick, timers use elapsed model
time.

Usage (stdlib only):
  python3 -I specs/zone_pressure_requests/reference.py          # self-check, write vectors
  python3 -I specs/zone_pressure_requests/reference.py --check  # self-check only
"""
import json
import os
import sys

EPS = 1e-9
STEP = 60.0

P = dict(
    dam_open=0.95, dam_band=0.10,   # damper "open" above 0.95, until no longer above 0.85
    open_for=60.0,                  # s the damper must stay open before airflow requests count
    flow_on=0.01, flow_on_band=0.005,  # m3/s; the zone is served while its setpoint is above
    three=0.5, two=0.7,             # airflow fractions of setpoint below which 3 or 2 requests
    flow_band=0.01,                 # m3/s release band on the airflow comparisons
)


class Above:
    """True once u exceeds t; stays true until u is no longer above t - band."""

    def __init__(self, t, band):
        self.t, self.band, self.y = t, band, False

    def step(self, u):
        self.y = u > self.t - self.band if self.y else u > self.t
        return self.y


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


class ZoneRequests:
    def __init__(self, p=P):
        self.p = p
        self.open = Above(p["dam_open"], p["dam_band"])
        self.long_open = TrueDelay(p["open_for"])
        self.served = Above(p["flow_on"], p["flow_on_band"])
        # Starved below a fraction of setpoint: the difference (fraction x setpoint - airflow)
        # goes above 0, and the condition holds until it is no longer above -flow_band.
        self.three = Above(0.0, p["flow_band"])
        self.two = Above(0.0, p["flow_band"])

    def step(self, t, dam, flow, setpoint):
        p = self.p
        is_open = self.open.step(dam)
        long_open = self.long_open.step(t, is_open)
        served = self.served.step(setpoint)
        three = self.three.step(p["three"] * setpoint - flow)
        two = self.two.step(p["two"] * setpoint - flow)
        counts = long_open and served
        if counts and three:
            return 3
        if counts and two:
            return 2
        return 1 if is_open else 0


def run(stim, horizon):
    model, out, k = ZoneRequests(), [], 0
    while k * STEP <= horizon + EPS:
        t = k * STEP
        dam, flow, setpoint = stim(t)
        out.append((t, (dam, flow, setpoint), model.step(t, dam, flow, setpoint)))
        k += 1
    return out


def piecewise(*pieces):
    """A stimulus from (start, value) pairs, sorted by start, the first at 0."""
    def at(t):
        value = pieces[0][1]
        for start, v in pieces:
            if t + EPS >= start:
                value = v
        return value
    return at


# Scenarios: (description, stimulus (damper, airflow, setpoint), horizon, checkpoints {t: y})
# with checkpoints worked out by hand from the behaviour, independent of the code above.
def scenarios():
    def stim(dam, flow, setpoint):
        return lambda t: (dam(t), flow(t), setpoint(t))

    const = lambda v: (lambda t: v)  # noqa: E731
    return {
        "damper_partly_open": (
            "Damper at 50 %, airflow at its setpoint: no requests.",
            stim(const(0.5), const(0.5), const(0.5)), 600, {0: 0, 600: 0}),
        "damper_open_hysteresis": (
            "Airflow at its setpoint; the damper opens to 96 % at 120 s (1 request), eases to 90 % "
            "at 300 s (still 1: not yet at or below 85 %), drops to exactly 85 % at 480 s (0), "
            "returns to exactly 95 % at 600 s (still 0: not above 95 %), then 95.1 % at 720 s (1).",
            stim(piecewise((0, 0.5), (120, 0.96), (300, 0.90), (480, 0.85), (600, 0.95),
                           (720, 0.951)), const(0.5), const(0.5)),
            900, {60: 0, 120: 1, 300: 1, 420: 1, 480: 0, 600: 0, 660: 0, 720: 1, 900: 1}),
        "starved_three": (
            "Damper fully open from the start, airflow 0.2 of a 0.5 m3/s setpoint (below half): 1 "
            "request at once, 3 once the damper has been open 60 s.",
            stim(const(1.0), const(0.2), const(0.5)), 600, {0: 1, 60: 3, 600: 3}),
        "starved_two": (
            "Damper fully open, airflow 0.3 of a 0.5 m3/s setpoint (below 70 % but not below "
            "half): 2 requests once the damper has been open 60 s.",
            stim(const(1.0), const(0.3), const(0.5)), 600, {0: 1, 60: 2, 600: 2}),
        "airflow_recovers_through_the_bands": (
            "Damper open, setpoint 0.5 m3/s; airflow rises 0.2, 0.255 (still within 0.01 of "
            "half: 3), 0.27 (2), 0.355 (still within 0.01 of 70 %: 2), 0.37 (1), then falls back "
            "to 0.34 (2) and 0.24 (3).",
            stim(const(1.0), piecewise((0, 0.2), (300, 0.255), (420, 0.27), (540, 0.355),
                                       (660, 0.37), (780, 0.34), (900, 0.24)), const(0.5)),
            1080, {240: 3, 300: 3, 420: 2, 540: 2, 660: 1, 780: 2, 900: 3, 1080: 3}),
        "zone_not_served": (
            "Damper open and no airflow, but the setpoint is 0 (the zone is off): only the "
            "damper request. When the setpoint rises to 0.5 m3/s at 300 s the starved zone sends "
            "3; at 0.008 m3/s from 600 s it is still served (not at or below 0.005), and at "
            "0.004 m3/s from 780 s it is not.",
            stim(const(1.0), const(0.0),
                 piecewise((0, 0.0), (300, 0.5), (600, 0.008), (780, 0.004))),
            960, {0: 1, 240: 1, 300: 3, 600: 3, 720: 3, 780: 1, 960: 1}),
        "reopen_restarts_the_timer": (
            "A starved zone whose damper is open from the start, closes to 80 % at 120 s and "
            "reopens at 300 s: 1 request at once, 3 from 60 s, 0 while closed, 1 at the reopen, "
            "and 3 again only after another 60 s.",
            stim(piecewise((0, 1.0), (120, 0.8), (300, 1.0)), const(0.1), const(0.5)),
            600, {0: 1, 60: 3, 120: 0, 240: 0, 300: 1, 360: 3, 600: 3}),
    }


def self_check():
    ok = True
    for name, (_, stim, horizon, checks) in scenarios().items():
        trace = {t: y for t, _, y in run(stim, horizon)}
        for t, want in checks.items():
            if trace[float(t)] != want:
                ok = False
                print(f"FAIL {name} at t={t}: y={trace[float(t)]} want {want}")
    print("self-check", "PASS" if ok else "FAIL")
    return ok


def windows(trace):
    out, start, value, last = [], None, None, None
    for t, _, v in trace:
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
        for key, i in (("uDam", 0), ("VDis_flow", 1), ("VSet_flow", 2)):
            steps, prev = [], object()
            for t, stim_vals, _ in trace:
                v = float(stim_vals[i])
                if v != prev:
                    steps.append({"t": t, "value": v})
                    prev = v
            inputs[key] = steps[0]["value"] if len(steps) == 1 else steps
        expect = [{"output": "yPreReq", "from_s": a, "to_s": b, "equals": v}
                  for a, b, v in windows(trace)]
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
