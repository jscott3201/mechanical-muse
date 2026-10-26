"""Independent reference for the chilled-water plant reset, and the generator of vectors.json.

Written from the behaviour of the Modelica Buildings Library's PlantReset sequence at commit
a131864e (trim and respond with a staging hold, then two clamped linear maps), read as block
semantics. It does not use open-control-engine code or engine output: the vectors it writes are
the oracle the engine-run graph is checked against.

Frame semantics match the engine's host tick: one evaluation per tick; sampled blocks fire on the
first tick at or after k * sample period (grid anchored at t = 0); timers use elapsed model time.

Usage (stdlib only):
  python3 -I specs/chw_plant_reset/reference.py          # self-check, then write vectors.json
  python3 -I specs/chw_plant_reset/reference.py --check  # self-check only
"""
import json
import os
import sys

EPS = 1e-9
INF = float("inf")
STEP = 60.0

# Chilled-water parameters (see spec.md). At r = 1 the outputs equal the BOPTEST baseline.
P = dict(
    TSup_nominal=278.71,  # K, design (coldest) supply temperature, used at full reset
    TSupSetLim=282.15,    # K, warmest allowed supply temperature
    dpSet_max=239125.0,   # Pa, design differential pressure
    dpSet_min=95650.0,    # Pa, lowest allowed differential pressure
    resDp_max=0.5,        # reset value at which dp reaches its maximum
    resTSup_min=0.5,      # reset value below which the temperature sits at its limit
    res_init=1.0, res_min=0.0, res_max=1.0,
    dtDel=900.0, dtRes=300.0, dtHol=900.0,
    nReqResIgn=2, tri=-0.02, rsp=0.03, rsp_max=0.07,
)


class TrueDelay:
    """Output true once the input has been true for `delay` seconds (delay counted from the
    first evaluation when the input starts true)."""

    def __init__(self, delay):
        self.d, self.first, self.u_prev, self.t_rise = delay, True, False, None

    def step(self, t, u):
        if self.first or (u and not self.u_prev):
            self.t_rise = t
        self.first, self.u_prev = False, u
        return u and t - self.t_rise >= self.d - EPS


class TrueFalseHold:
    """Output follows the input, but a true value is held for at least `true_hold` seconds and a
    false value for at least `false_hold` seconds."""

    def __init__(self, true_hold, false_hold):
        self.th, self.fh = true_hold, false_hold
        self.y, self.first, self.entered = None, True, -INF

    def step(self, t, u):
        if self.first:
            self.first, self.y, self.entered = False, u, t
            return u
        held = self.th if self.y else self.fh
        if u != self.y and t >= self.entered + held - EPS:
            self.y, self.entered = u, t
        return self.y


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


def line(u, x1, f1, x2, f2):
    """Linear map through (x1, f1) and (x2, f2), with u clamped to [x1, x2]."""
    b = (f2 - f1) / (x2 - x1)
    return f2 - b * x2 + b * min(x2, max(x1, u))


class PlantReset:
    def __init__(self, p=P):
        self.p = p
        self.delay = TrueDelay(p["dtDel"] + p["dtRes"])
        self.hold = TrueFalseHold(p["dtHol"], 0.0)
        self.clock = Clock(p["dtRes"])
        self.requests = 0.0             # requests as sampled at the last tick
        self.prev = p["res_init"]       # reset value stored at the last sample
        self.stored = p["res_init"]
        self.frozen = False             # inside a staging hold, as seen at the last sample

    def step(self, t, n_req, enabled, staging):
        p = self.p
        active = self.delay.step(t, enabled)
        holding = self.hold.step(t, staging)
        sample = self.clock.fire(t)
        if sample:
            self.requests = float(n_req)
            self.prev = self.stored
            self.frozen = holding
        excess = self.requests - p["nReqResIgn"]
        if active and excess > 0:
            change = p["tri"] + min(excess * abs(p["rsp"]), abs(p["rsp_max"]))
        else:
            change = p["tri"] if active else 0.0
        if self.frozen:
            change = 0.0
        value = max(min(self.prev + change, p["res_max"]), p["res_min"])
        if not enabled:
            value = p["res_init"]
        if sample:
            self.stored = value
        r = value if active else p["res_init"]
        T = line(r, p["resTSup_min"], p["TSupSetLim"], 1.0, p["TSup_nominal"])
        dp = line(r, 0.0, p["dpSet_min"], p["resDp_max"], p["dpSet_max"])
        return r, T, dp


def run(stim, horizon):
    model, out, k = PlantReset(), [], 0
    while k * STEP <= horizon + EPS:
        t = k * STEP
        n_req, enabled, staging = stim(t)
        out.append((t, (n_req, enabled, staging), model.step(t, n_req, enabled, staging)))
        k += 1
    return out


# Scenarios: (description, stimulus, horizon, checkpoints {t: r}) with checkpoints worked out by
# hand from the behaviour, independent of the code above.
def scenarios():
    def stepped(t):
        for start, n in ((5400, 10), (4800, 5), (4200, 4), (3600, 3), (3000, 2)):
            if t >= start:
                return n
        return 0

    return {
        "trim_from_enable": (
            "Plant enabled from the start, no requests. Nothing changes for the start delay "
            "plus one sample period (1200 s); then the reset trims 2 % every 300 s, warming the "
            "supply water first and lowering the pressure setpoint only after the temperature "
            "reaches its limit.",
            lambda t: (0, True, False), 17100,
            {0: 1.0, 1140: 1.0, 1200: 0.98, 1500: 0.96, 8400: 0.5, 15900: 0.0, 17100: 0.0}),
        "ignore_respond_cap": (
            "Requests step 0, 2, 3, 4, 5, 10. Up to 2 requests are ignored; above that each "
            "extra request adds 3 % before the 2 % trim, capped at 7 %.",
            lambda t: (stepped(t), True, False), 6600,
            {2700: 0.88, 3000: 0.86, 3300: 0.84, 3600: 0.85, 3900: 0.86, 4200: 0.90,
             4500: 0.94, 4800: 0.99, 5100: 1.0, 6600: 1.0}),
        "ceiling_clamp": (
            "Three requests from the start: the net change is +1 %, but the reset cannot rise "
            "above 100 %, so the setpoints stay at design.",
            lambda t: (3, True, False), 6000, {1200: 1.0, 6000: 1.0}),
        "saturate_then_release": (
            "Five requests until 6000 s push the reset against its 100 % ceiling; when they "
            "stop, trimming starts from 100 % at the next sample, not from some value above it.",
            lambda t: (5 if t < 6000 else 0, True, False), 7200,
            {1200: 1.0, 5940: 1.0, 6000: 0.98, 6300: 0.96, 7200: 0.9}),
        "hold_short_staging": (
            "Staging for 600 s from 3000 s. Updates freeze for the full 900 s hold after the "
            "staging starts, then resume.",
            lambda t: (0, True, 3000 <= t < 3600), 4800,
            {2700: 0.88, 3000: 0.88, 3600: 0.88, 3840: 0.88, 3900: 0.86, 4200: 0.84}),
        "hold_long_staging": (
            "Staging for 1800 s from 3000 s, longer than the 900 s hold: updates freeze for as "
            "long as staging lasts.",
            lambda t: (0, True, 3000 <= t < 4800), 5400,
            {3000: 0.88, 4740: 0.88, 4800: 0.86, 5100: 0.84}),
        "staging_retrigger": (
            "Two staging changes 1,200 s apart (3000-3300 s and 4200-4500 s). Each start "
            "freezes updates for its own 900 s hold, so the update at 4200 s is frozen even "
            "though the first hold has ended; updates resume at 5100 s.",
            lambda t: (0, True, 3000 <= t < 3300 or 4200 <= t < 4500), 6000,
            {3000: 0.88, 3600: 0.88, 3900: 0.86, 4200: 0.86, 4800: 0.86, 5100: 0.84,
             5400: 0.82}),
        "off_grid_edges": (
            "Staging and request changes between update instants: staging from 3030 to 3090 s "
            "and from 4020 to 4080 s, and four requests from 5130 s. A hold that starts between "
            "updates still freezes the next updates; requests that arrive between updates are "
            "counted at the next update (5400 s).",
            lambda t: (4 if t >= 5130 else 0, True, 3030 <= t < 3090 or 4020 <= t < 4080),
            6600, {3000: 0.86, 3300: 0.86, 3900: 0.86, 4200: 0.86, 4800: 0.86, 5100: 0.84,
                   5400: 0.88}),
        "disable_and_reenable": (
            "The plant is disabled from 9000 s to 9900 s: the setpoints return to design at "
            "once, and the start delay begins again at re-enable.",
            lambda t: (0, not (9000 <= t < 9900), False), 11700,
            {8700: 0.48, 9000: 1.0, 11040: 1.0, 11100: 0.98, 11400: 0.96}),
        "recover_from_floor": (
            "Trim to the floor, then five requests from 16200 s: the pressure setpoint is "
            "restored first, then the supply temperature is lowered back to design.",
            lambda t: (0 if t < 16200 else 5, True, False), 22200,
            {15900: 0.0, 16200: 0.05, 18900: 0.5, 19200: 0.55, 21900: 1.0, 22200: 1.0}),
    }


def self_check():
    ok = True
    for name, (_, stim, horizon, checks) in scenarios().items():
        trace = {t: rtd[0] for t, _, rtd in run(stim, horizon)}
        for t, want in checks.items():
            if abs(trace[float(t)] - want) > 1e-9:
                ok = False
                print(f"FAIL {name} at t={t}: r={trace[float(t)]} want {want}")
    for r, T, dp in ((1.0, 278.71, 239125.0), (0.5, 282.15, 239125.0), (0.0, 282.15, 95650.0)):
        got = (line(r, P["resTSup_min"], P["TSupSetLim"], 1.0, P["TSup_nominal"]),
               line(r, 0.0, P["dpSet_min"], P["resDp_max"], P["dpSet_max"]))
        if abs(got[0] - T) > 1e-9 or abs(got[1] - dp) > 1e-6:
            ok = False
            print(f"FAIL map r={r}: {got}")
    print("self-check", "PASS" if ok else "FAIL")
    return ok


def windows(trace, index, tol):
    """Compress a per-tick output into inclusive windows of equal value."""
    out, start, value, last = [], None, None, None
    for t, _, outputs in trace:
        v = outputs[index]
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
        for key, i, cast in (("nReqRes", 0, int), ("u1Ena", 1, bool), ("u1StaPro", 2, bool)):
            steps, prev = [], object()
            for t, stim_vals, _ in trace:
                v = cast(stim_vals[i])
                if v != prev:
                    steps.append({"t": t, "value": v})
                    prev = v
            inputs[key] = steps[0]["value"] if len(steps) == 1 else steps
        expect = []
        for output, index, tol in (("TSupSet", 1, 1e-6), ("dpSet", 2, 1e-3)):
            for a, b, v in windows(trace, index, 1e-9):
                expect.append({"output": output, "from_s": a, "to_s": b,
                               "equals": round(v, 9), "tolerance": tol})
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
