# Energy and emissions ledger

Every BOPTEST run (live, or a recorded run) gets a ledger: how much energy the HVAC plant used,
the emissions that energy stands for, and what it cost, split into Scope 1 (gas burnt on site)
and Scope 2 (electricity bought). Each test case has its own basis of BOPTEST's constants
(`crates/mm-runtime/src/ledger/basis.rs`): the office below, and the home (an all-electric house
on spot prices, see [The home](#the-home)). Two runs of the same window can be compared. The numbers are
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
`reconciles` is true when it does (to float rounding) and neither carrier is negative. All thirteen
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

## The home

From BOPTEST v0.9.0, test case `bestest_hydronic_heat_pump`:

| Quantity | Value | BOPTEST file |
|---|---|---|
| Electricity emission factor | 0.167 kgCO2/kWh, the same every hour | `models/Resources/emissions.csv` (`EmissionsElectricPower`) |
| Gas | none: the heat pump runs on electricity, so there is no Scope 1 | `models/kpis.json` (`ElectricPower` only) |
| Electricity price | spot prices with fixed fees, in euros, varying hour by hour | `models/Resources/prices.csv` (`PriceElectricPowerHighlyDynamic`, the scenario the runtime selects) |
| Floor area | 192 m2 | `models/config.json` (`area`) |
| End uses | heat pump compressor, heat pump fan, floor heating pump | `models/kpis.json` (`reaPHeaPum_y`, `reaPFan_y`, `reaPPumEmi_y`) |

Differences from the office:

- **All electric.** `ener_tot` is all electricity; Scope 1 is shown as none rather than zero.
- **Cost is BOPTEST's own.** With prices that change every hour the energy cannot be re-priced
  at a constant rate, so the check is on emissions instead: BOPTEST's `emis_tot` must equal the
  energy times 0.167 (it does, to rounding, on all four committed home records). End uses are
  priced at the run's average price paid, and the ledger reports that average.
- **Timing changes cost, not emissions.** BOPTEST gives this house one emission factor for the
  whole year, so moving heat out of expensive hours lowers the bill and the average price paid
  but cuts emissions only by the energy it saves. Typical heat window, price response against
  BOPTEST's controller: 340.4 → 314.3 kWh, 56.9 → 52.5 kgCO2e (−7.7 %), €79.21 → €72.59, average
  €0.2327 → €0.2310 per kWh. Peak heat window: 667.7 → 617.5 kWh, 111.5 → 103.1 kgCO2e
  (−7.5 %), €174.47 → €160.02, average €0.2613 → €0.2592. A grid whose carbon intensity varies
  through the day would reward the shift on emissions too; this simulation does not model that.

## Comparisons and test faults

A comparison is `b - a`, refused (`window-mismatch`) unless both runs cover the same test case,
start, step length, steps asked for and ticks recorded. The run view compares a run with a
baseline of the same window (BOPTEST's own controllers, nothing written) when one exists, and
sets thermal discomfort (`tdis_tot`) beside the savings, so a saving bought with discomfort shows.

A test fault (`docs/runtime-results.md`) changes one reading the sequences see: blank (NaN),
stuck at a value, or biased by an offset. Nothing is written to BOPTEST for the faulted reading
itself; the setpoints the sequences compute from it still reach BOPTEST, within their clamps,
which is how a fault costs energy. A held value on a point the plant did not report in a step
invents a labelled reading for that step, so the stale-data check does not fire for it. When a
run with a fault has a twin of the same window without it (same switches, same drafted graphs by
content id), the run view prices the fault against that twin; without one it only labels the
fault.

### A fault's cost, measured (2026-10-27)

BOPTEST on Modal, day 185 from noon, 300 steps of 120 s (10 h), our hand-built AHU requests
feeding the chilled-water plant reset. Two runs, alike except for a test fault: floor 1's
supply-air temperature read 4 K high from step 30 for 60 steps (2 h), as a drifted sensor would.
Records: `fixtures/replays/day185-noon-healthy.jsonl` and
`day185-noon-fault-sensor-bias.jsonl` (both with every power reading, so end uses show).

- 4 K above setpoint makes floor 1's air handler send 3 requests, more than the 2 the plant
  reset ignores, so the reset drove the chilled-water setpoint to its cold end: 5.56 °C during
  the fault against 9.00 °C in the healthy run. It recovered about 70 steps after the reading did.
- Against the healthy run: +130 kWh, +44 kgCO2e and +$12.25, nearly all Scope 2. Chillers
  +147 kWh, cooling towers +6, chilled-water pumps −23; gas unchanged. Thermal discomfort was
  slightly lower (colder water).
- For scale: the healthy run cost $912 against $972 for BOPTEST's baseline over a baseline run of
  the same window (a separate record), so two hours of one bad sensor gave back about a fifth of
  the ten hours' saving.
- End uses on these live records reconcile with BOPTEST's totals to within 3.4 kWh of
  13,261 kWh (the "not in the 120 s samples" rows).

One window, one fault, in simulation. The cost is measured as the difference of two runs that
BOPTEST computed, not estimated from a formula.

Muse Spark's explanation of the fault replay, written live with the ledger as given facts, is
recorded (`fixtures/explanations/run_4520f56cccb2b064.json`) and served, labelled recorded,
without a key. It quotes the cost (+44 kg, +$12, +130 kWh against the run without the fault)
and passes the number check.

Run explanations get the ledger and these comparisons as given facts, rounded, and are told to
quote rather than compute. A reply that states a number not in its input (beyond a rounded
quote, a duration from step counts, or a small count) is sent back once naming those numbers,
then refused (`mm_agent::unsupported_numbers`). A replay whose events already have a recorded
explanation (written before ledger facts existed) is served that recording, which makes no
energy claims; it is not rewritten with the ledger.

## What this is not

- **A simulation, not an inventory.** These are BOPTEST's emulated building and weather over a
  chosen window, not metered data from a real building, and not a GHG inventory or report.
- **BOPTEST's average factors, not a utility's.** The factors are the test case's own constants;
  a real site's grid factor, supplier mix or hourly marginal emissions would give other numbers.
- **HVAC only, Scopes 1 and 2 only.** No lighting or plug loads, no refrigerant leakage, and no
  Scope 3.
