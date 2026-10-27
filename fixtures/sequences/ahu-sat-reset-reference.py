"""Independent reference for ahu-sat-reset-sample.md, written by hand from the sheet (not from
any model output), used to grade the scenarios Muse Spark proposes for it.

It reads a reading (the server's ReadView, e.g. fixtures/readings/read_*.json, or the
`make live-read` output in runs/reads/), finds the points by their meaning, simulates every
scenario on its clock and compares every checked instant. Where the sheet is silent it tries
each reading separately (strict or inclusive 60 % threshold; whether the dehumidification flag
can be on while the AHU is off) and reports agreement for each one on its own.

usage: python3 fixtures/sequences/ahu-sat-reset-reference.py <reading.json> [--rev b]
(--rev b for ahu-sat-reset-sample-v2.md: dehumidification on above 70 %, off below 65 %)
"""
import json, sys

REV_B = "--rev" in sys.argv and sys.argv[-1].lower() == "b"
RH_ON, RH_OFF = (70, 65) if REV_B else (60, 55)

F = lambda f: (f - 32) * 5 / 9 + 273.15
T55, T65, OA_HI, OA_LO = F(55), F(65), F(70), F(60)


def guess(points, words):
    for p in points:
        text = (p["name"] + " " + p["meaning"]).lower()
        if all(w in text for w in words):
            return p
    return None


def value_at(spec, t):
    if not isinstance(spec, list):
        return spec
    v = spec[0]["value"]
    for s in spec:
        if s["t"] <= t:
            v = s["value"]
    return v


def simulate(inputs, names, clock, strict_rh=True, dehum_when_off=False, rh_scale=1.0):
    step, horizon = clock["step_s"], clock["horizon_s"]
    ena_n, oat_n, rh_n = names
    out, t = [], 0.0
    on_since, prev_on, high_since, dehum = None, False, None, False
    while t <= horizon + 1e-9:
        on = bool(value_at(inputs[ena_n], t))
        oat = float(value_at(inputs[oat_n], t))
        rh = float(value_at(inputs[rh_n], t)) * rh_scale
        if on and not prev_on:
            on_since = t
        prev_on = on
        high = rh > RH_ON if strict_rh else rh >= RH_ON
        if high:
            high_since = t if high_since is None else high_since
        else:
            high_since = None
        if not dehum and high_since is not None and t - high_since >= 300:
            dehum = True
        if dehum and rh < RH_OFF:
            dehum = False
        active = dehum and (on or dehum_when_off)
        if not on:
            sp = T55
        elif t - on_since < 600:
            sp = T55
        elif active:
            sp = T55
        else:
            x = min(max((oat - OA_LO) / (OA_HI - OA_LO), 0.0), 1.0)
            sp = T65 + (T55 - T65) * x
        out.append((t, sp, active))
        t += step
    return out


def from_view(v):
    """A server ReadView (draft with inputs as lists of steps) in the reader's own shape."""
    d = v["draft"]
    const = lambda s: s[0]["value"] if len(s) == 1 and s[0]["t"] == 0 else s
    return {
        "task": {"inputs": d["inputs"], "outputs": d["outputs"]},
        "vectors": {
            "clock": {"step_s": d["step_s"], "horizon_s": d["horizon_s"]},
            "scenarios": [
                {"name": s["name"], "inputs": {i["point"]: const(i["steps"]) for i in s["inputs"]}, "expect": s["expect"]}
                for s in d["scenarios"]
            ],
        },
    }


def main():
    r = json.load(open(sys.argv[1]))
    if "draft" in r:
        r = from_view(r)
    task, vec = r["task"], r["vectors"]
    ena = guess(task["inputs"], ["enable"]) or guess(task["inputs"], ["on"])
    oat = guess(task["inputs"], ["outdoor"])
    rh = guess(task["inputs"], ["humid"])
    sp = guess(task["outputs"], ["setpoint"])
    dh = guess(task["outputs"], ["dehum"])
    print("mapping:", [p and p["name"] for p in (ena, oat, rh, sp, dh)])
    # Humidity given as a fraction instead of percent?
    rh_unit = rh.get("unit", "")
    scale = 100.0 if rh_unit in ("1", "fraction") else 1.0
    variants = {
        "strict threshold, dehum off when AHU off": dict(strict_rh=True, dehum_when_off=False),
        "inclusive threshold": dict(strict_rh=False, dehum_when_off=False),
        "dehum flag also while off": dict(strict_rh=True, dehum_when_off=True),
    }
    for vname, kw in variants.items():
        agree = disagree = 0
        notes = []
        for s in vec["scenarios"]:
            sim = simulate(s["inputs"], (ena["name"], oat["name"], rh["name"]), vec["clock"], rh_scale=scale, **kw)
            for e in s["expect"]:
                for t, spv, act in sim:
                    if not (e["from_s"] - 1e-9 <= t <= e["to_s"] + 1e-9):
                        continue
                    if e["output"] == sp["name"]:
                        ok = abs(spv - e["equals"]) <= max(e.get("tolerance") or 1e-6, 1e-3)
                        got = round(spv, 4)
                    elif dh and e["output"] == dh["name"]:
                        ok = act == e["equals"]
                        got = act
                    else:
                        continue
                    if ok:
                        agree += 1
                    else:
                        disagree += 1
                        notes.append(f"{s['name']} t={t:.0f} {e['output']}: proposed {e['equals']} reference {got}")
        print(f"\n[{vname}] ticks agree {agree}, disagree {disagree}")
        seen = set()
        for n in notes:
            key = n.split(" t=")[0] + n.split(":")[1].split(" reference")[0]
            if key in seen:
                continue
            seen.add(key)
            print("  ", n)


main()
