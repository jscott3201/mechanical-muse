"""Independent reference for one zone's static pressure reset requests (v2), and the generator of
vectors.json.

v1 followed the static pressure half of the Modelica Buildings Library's VAV terminal
system-request sequence (commit a131864e, as vendored by the pinned open-control-engine): airflow
requests only once the damper had been above 95 % for a minute. On BOPTEST the zone dampers open
too slowly for that (spec.md), so v2 keeps the damper request and adds requests from the airflow
shortfall itself. Read as elementary block semantics: one-sided hysteresis comparisons and
true-delays. It does not use open-control-engine code or engine output: the vectors it writes are
the oracle the engine-run graph is checked against. v1's task and vectors are kept as
task.v1.json and vectors.v1.json.

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
    flow_on=0.01, flow_on_band=0.005,  # m3/s; the zone is served while its setpoint is above
    one=0.9, two=0.7, three=0.5,    # airflow fractions of setpoint below which 1, 2 or 3 requests
    flow_band=0.01,                 # m3/s release band on the airflow comparisons
    persist=120.0,                  # s an airflow shortfall must last before it counts
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
        self.served = Above(p["flow_on"], p["flow_on_band"])
        # Short of a fraction of setpoint: the difference (fraction x setpoint - airflow) goes
        # above 0, and the condition holds until it is no longer above -flow_band.
        self.short = {k: Above(0.0, p["flow_band"]) for k in ("one", "two", "three")}
        self.long = {k: TrueDelay(p["persist"]) for k in ("one", "two", "three")}

    def step(self, t, dam, flow, setpoint):
        p = self.p
        is_open = self.open.step(dam)
        served = self.served.step(setpoint)
        lasting = {}
        for k in ("one", "two", "three"):
            short = self.short[k].step(p[k] * setpoint - flow)
            lasting[k] = self.long[k].step(t, short and served)
        if lasting["three"]:
            return 3
        if lasting["two"]:
            return 2
        return 1 if lasting["one"] or is_open else 0


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
            "Damper at 60 % (not open), airflow 0.2 of a 0.5 m3/s setpoint (below half) from the "
            "start: nothing until the shortfall has lasted 120 s, then 3 requests.",
            stim(const(0.6), const(0.2), const(0.5)), 600, {0: 0, 60: 0, 120: 3, 600: 3}),
        "starved_two": (
            "Airflow 0.3 of a 0.5 m3/s setpoint (below 70 %, not below half): 2 requests after "
            "120 s.",
            stim(const(0.6), const(0.3), const(0.5)), 600, {60: 0, 120: 2, 600: 2}),
        "short_one": (
            "Airflow 0.425 of a 0.5 m3/s setpoint (below 90 %, not below 70 %): 1 request after "
            "120 s.",
            stim(const(0.6), const(0.425), const(0.5)), 600, {60: 0, 120: 1, 600: 1}),
        "airflow_recovers_through_the_bands": (
            "Setpoint 0.5 m3/s; airflow 0.2 (3 after 120 s), 0.255 at 300 s (still within 0.01 of "
            "half: 3), 0.27 (2), 0.355 (still within 0.01 of 70 %: 2), 0.37 (1), 0.455 (still "
            "within 0.01 of 90 %: 1), 0.47 (0); then back to 0.34 at 1020 s: the shortfalls start "
            "again, so 2 only from 1140 s.",
            stim(const(0.6), piecewise((0, 0.2), (300, 0.255), (420, 0.27), (540, 0.355),
                                       (660, 0.37), (780, 0.455), (900, 0.47), (1020, 0.34)),
                 const(0.5)),
            1260, {120: 3, 300: 3, 420: 2, 540: 2, 660: 1, 780: 1, 900: 0, 1020: 0, 1080: 0,
                   1140: 2, 1260: 2}),
        "zone_not_served": (
            "Damper at 60 % and no airflow, but the setpoint is 0 (the zone is off): no requests. "
            "When the setpoint rises to 0.5 m3/s at 300 s the starved zone sends 3 from 420 s; at "
            "0.008 m3/s from 600 s it is still served (not at or below 0.005), and at 0.004 m3/s "
            "from 780 s it is not.",
            stim(const(0.6), const(0.0),
                 piecewise((0, 0.0), (300, 0.5), (600, 0.008), (780, 0.004))),
            960, {0: 0, 300: 0, 360: 0, 420: 3, 600: 3, 720: 3, 780: 0, 960: 0}),
        "brief_dips_ignored": (
            "Airflow at its setpoint except dips to 60 % for 60 s (300 s), 120 s (600 s) and "
            "180 s (900 s): a dip counts only once it has lasted 120 s, so only the last one "
            "sends 2 requests, at 1020 s.",
            stim(const(0.6), piecewise((0, 0.5), (300, 0.3), (360, 0.5), (600, 0.3), (720, 0.5),
                                       (900, 0.3), (1080, 0.5)), const(0.5)),
            1200, {300: 0, 600: 0, 660: 0, 720: 0, 900: 0, 960: 0, 1020: 2, 1080: 0, 1200: 0}),
        "damper_and_airflow": (
            "Damper at 96 % with airflow at 85 % of setpoint: 1 request at once from the damper, "
            "still 1 after 120 s; airflow falls to 60 % at 300 s, so 2 from 420 s.",
            stim(const(0.96), piecewise((0, 0.425), (300, 0.3)), const(0.5)),
            600, {0: 1, 120: 1, 300: 1, 360: 1, 420: 2, 600: 2}),
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
