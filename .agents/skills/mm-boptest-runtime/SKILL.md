---
name: mm-boptest-runtime
description: "Guides mm-runtime: BOPTEST REST loop, 120 s step, clamps, derived signals, fail-safe, KPIs. Use when changing the supervisory loop, point writes, the fake plant, KPI code or BOPTEST setup in compose."
license: MIT OR Apache-2.0
metadata:
  short-description: "BOPTEST loop: REST flow, clamps, fail-safe, KPIs"
---
Test case `multizone_office_complex_air` (15 VAV boxes, floor 2 is x10, 386 points). Depth: `docs/boptest-point-map.md` sections 3, 6-8, 10; `docs/plant-reset.md`.

**REST** (BOPTEST v0.9.0 compose serves 127.0.0.1:80; replies are `{status,message,payload}`)
- Send `Content-Type: application/json` only with a body: a bodiless request carrying it gets a 500 (the server parses the empty body).
- `POST /testcases/multizone_office_complex_air/select` gives a testid. Then `PUT /scenario/{id}` with `{"time_period":"peak_heat_day"}` (warm-up runs under baseline control) or `PUT /initialize/{id}`; `PUT /step/{id}` `{"step":120}`; loop `POST /advance/{id}`; `GET /kpi/{id}`; `PUT /stop/{id}`. An empty payload means the scenario ended. An idle test stops after 900 s.
- Local service: `make boptest-up` (`compose.yaml`, needs `BOPTEST_DIR`). One worker, emulated x86 on Apple silicon: select about 2.5 min, initialize 7-19 min (mostly fixed), a 120 s step 0.8 s (summer morning) to 9.4 s (January). Day 184 is a Sunday; use day 185 for an occupied summer weekday. Results: `docs/runtime-results.md`. Always stop a test: an orphan holds the only worker. `/stop` replies with a plain `OK`, not JSON. Details in `docs/DECISIONS.md`.
- Live test: `cargo test -p mm-runtime --test live_boptest -- --ignored --nocapture` (env knobs in the file header).

**Each 120 s step** (10,080 per two weeks)
1. Take the last payload (measurements plus the effective `_u` values), refresh derived signals, run one engine frame at `t` = payload `time` (`mm-oce-engine-host`).
2. Clamp and gate the 10 outputs, then send `<name>_u` and `<name>_activate`=1 for each written point on every step, and an explicit `_activate`=0 for a released one (activation is sticky). Never send a derived name: an unknown key is a 400 and no step happens.
3. K and Pa end to end, no conversion. BOPTEST only warns when it truncates an out-of-range write and passes NaN, so clamp and reject non-finite values yourself.

- Clamp (baseline in parentheses; K and Pa): `floorN_TSupAirSet_u` 285.15..313.15 (285.95); `floorN_dpSet_u` 50..410 (400); `oveChiWatSys_TWSet_u` 278.15..288.15 (278.71); `oveHotWatSys_TWSet_u` 291.15..353.15 (353.15); plant `dpSet_u` 0..478,250 (CHW 239,125; HW 119,562.5). BOPTEST's own 19.13 MPa plant dp bound is a placeholder, hence the guard.
- Gate: AHU points only while `floorN_reaAHU_occ_y` is 1, else release; plant points only while the derived plant-enable is true.

**Fail-safe** (the engine has none; AGENTS rule 5)
- A refused frame, NaN or stale sensor means no new setpoints: hold the last good values or release to baseline (`_activate`=0), emit a degraded event, keep advancing. Only a CXF with a green receipt ever runs. Test this path with a fake plant behind a `Plant` trait.

**Derived signals** (no BOPTEST point; label them derived in UI and BACnet): zone cooling loop `uCoo`, AHU fan status, operation mode (engine numbering, occupied = 1), reheat discharge setpoint (neutralized), plant enable, staging flag (constant false). HW requests come from the 15 reheat valves because the AHUs have no HW coil. Floor 2 flows and power read about 10x: label them beside floors 1 and 3.

**KPIs**: `GET /kpi` (`ener_tot`, `cost_tot`, `emis_tot`, `pele_tot`, `pgas_tot`, `tdis_tot`, `idis_tot`, `time_rat`); compare with a baseline-only run of the same scenario.
