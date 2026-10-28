# Energy and emissions ledger

Every BOPTEST run (live, or a recorded run) gets a ledger: how much energy the HVAC plant used,
the emissions that energy stands for, and what it cost, split into Scope 1 (gas burnt on site)
and Scope 2 (electricity bought). Two runs of the same window can be compared. The numbers are
simulation results; nothing reads them to decide a control action.

Where it lives: the computation is `crates/mm-runtime/src/ledger.rs` (pure, no I/O); the HTTP
view and the comparison are `crates/mm-server/src/ledger.rs`; routes are
`GET /api/runs/{id}/ledger`, `GET /api/replays/{name}/ledger` and
`GET /api/ledger/compare?a=&b=` (each of `a`, `b` is `run:<id>` or `replay:<record>`); the MCP
tools are `run_ledger` and `compare_runs` ([mcp.md](mcp.md)); the web app shows it in the run
view as "Energy & emissions".

## What is counted

Exactly what BOPTEST counts for its energy KPIs on `multizone_office_complex_air`: 13 electric
power signals and 1 gas signal (its `models/kpis.json`).

| Scope | End use | BOPTEST signals |
|---|---|---|
| 2 | Chillers | `reaChiWatSys_reaPChi_y` |
| 2 | Chilled-water pumps | `reaChiWatSys_reaPPum_y` |
| 2 | Cooling towers | `reaChiWatSys_reaPCooTow_y` |
| 2 | Supply fans | `floorN_reaAHU_PFanSup_y` (3 floors) |
| 2 | Return fans | `floorN_reaAHU_PFanRet_y` |
| 2 | Freeze-protection coils | `floorN_reaAHU_PFreCoi_y` |
| 2 | Hot-water pump | `reaHotWatSys_reaPPum_y` |
| 1 | Boiler | `reaHotWatSys_reaPBoi_y` |

Lighting and plug loads are internal heat gains in this test case, not KPI energy, so they are
not in the ledger. Peak demand KPIs (`pele_tot`, `pgas_tot`) are power, not energy, and are never
added in.

## Method

**Scope totals are exact.** BOPTEST reports total energy (`ener_tot`, kWh/m2) and total
emissions (`emis_tot`, kgCO2/m2), each integrated over its own fine output grid. Its emission
factors for this case are constant over the year, so the two totals give two linear equations in
the electricity and gas energy, and the split is exact:

    electricity = (emis_tot - 0.18108 * ener_tot) / (0.341 - 0.18108)
    gas         = ener_tot - electricity

both per m2, times the floor area for kWh. Emissions and cost follow from the factors and prices
below.

**Reconciliation.** Re-pricing the split must give back BOPTEST's own cost KPI (`cost_tot`).
`reconciles` is true when it does (to float rounding) and neither carrier is negative. All eleven
committed recorded runs reconcile.

**End uses come from our recorded readings.** Every tick records the 14 power signals (one
sample per 120 s step; three are in the trend set, eleven in the energy set of
`mm_runtime::preset`). Each is integrated with the trapezoid rule and BOPTEST's own J-to-kWh
factor, as `kpis/kpi_calculator.py` does. Two things BOPTEST sees and our samples do not land in
an "unattributed" row per scope, shown as its own line: what happens between 120 s samples, and
the last step (a tick holds the readings the step started from). The rows of a scope always add
up to its exact total.

**Older records** were made before the fan, freeze-coil, hot-water pump and boiler readings were
recorded. They get scope totals only (`measured: false`, with the missing signals listed), and
the web app says "end uses not recorded".

## Factors and prices

From BOPTEST v0.9.0, test case `multizone_office_complex_air`:

| Quantity | Value | BOPTEST file |
|---|---|---|
| Electricity emission factor | 0.341 kgCO2/kWh | `models/Resources/emissions.csv` (`EmissionsElectricPower`) |
| Gas emission factor | 0.18108 kgCO2/kWh | `models/Resources/emissions.csv` (`EmissionsGasPower`) |
| Electricity price | $0.094/kWh | `models/Resources/prices.csv` (`PriceElectricPowerConstant`) |
| Gas price | $0.024/kWh | `models/Resources/prices.csv` (`PriceGasPower`) |
| Floor area | 42,757 m2 | `models/config.json` (`area`) |
| Integration and KPI formulas | | `kpis/kpi_calculator.py` (`get_energy`, `get_cost`, `get_emissions`) |

Only the `Constant` price scenario is used; the runtime never selects another. BOPTEST also ships
dynamic price profiles, which this ledger does not use.

## Comparisons and test faults

A comparison is `b - a`, refused (`window-mismatch`) unless both runs cover the same test case,
start, step length, steps asked for and ticks recorded. The run view compares a run with a
baseline of the same window (BOPTEST's own controllers, nothing written) when one exists, and
sets thermal discomfort (`tdis_tot`) beside the savings, so a saving bought with discomfort shows.

A test fault (`docs/runtime-results.md`) changes one reading the sequences see: blank (NaN),
stuck at a value, or biased by an offset. It never writes to BOPTEST. When a run with a fault has
a twin of the same window without it, the run view prices the fault against that twin; without
one it only labels the fault. The scope label of a fault's cost follows the convention of the
library cards' `emissions.scope` field (1 for on-site fuel, 2 for purchased electricity), for
example the chilled-water cards CHW-0002 and CHW-0004 and the air handler cards AHU-0018 and
AHU-0024. We use that field as a label only, not the cards' own estimates.

Run explanations get the ledger and these comparisons as given facts, rounded, and are told to
quote rather than compute; a reply that states a number not in its input is refused
(`mm_agent::unsupported_numbers`).

## What this is not

- **A simulation, not an inventory.** These are BOPTEST's emulated building and weather over a
  chosen window, not metered data from a real building, and not a GHG inventory or report.
- **BOPTEST's average factors, not a utility's.** The factors are the test case's own constants;
  a real site's grid factor, supplier mix or hourly marginal emissions would give other numbers.
- **HVAC only, Scopes 1 and 2 only.** No lighting or plug loads, no refrigerant leakage, and no
  Scope 3.
