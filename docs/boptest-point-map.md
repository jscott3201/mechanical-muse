# BOPTEST point map and G36 wiring: `multizone_office_complex_air`

Prepared for Justin Scott (Aionforge Labs), project Mechanical Muse. Date: 2026-10-26.
Companion files in the same folder: `boptest-points.csv` (all 386 point names with unit, min, max, group, floor, zone, role) and `bacnet-device-table.md` / `bacnet-objects.csv` (BACnet side). For the plant-reset block itself see the other agent's `plant-reset.md` in this folder; I reuse its baseline facts and proposed values instead of repeating its analysis.

Method and limits: I read the BOPTEST clone (`testcases/multizone_office_complex_air/` models, `testcase.py`, `bacnet/`), the published case documentation (byte-identical to the repo copy), and the open-control-engine G36 fixtures plus vendored Modelica sources. **Nothing was executed against a live BOPTEST or a live engine.** Every statement about runtime behavior is a reading of source; section 10 lists what to check on the first run. Anything I inferred rather than read is marked "inferred" or "proposal".

## 0. Headline

- **Points.** 386 names: 204 measurements, 91 overwrite inputs `_u`, 91 `_activate` flags. 295 of them are data points a BACnet front end shows (204 + 91); the 91 `_activate` flags are derived from the BACnet priority array. Grouping: 3 AHUs (63 measurements, 27 inputs), 15 zones/VAV boxes (105 measurements, 60 inputs), CHW plant (7, 2), HW plant (6, 2), weather (23, 0).
- **Writes.** The supervisory loop writes 10 inputs: `floorN_TSupAirSet_u` and `floorN_dpSet_u` (N = 1..3) and `oveChiWatSys_TWSet_u`, `oveChiWatSys_dpSet_u`, `oveHotWatSys_TWSet_u`, `oveHotWatSys_dpSet_u`. All are in SI units that match the sequence outputs (K, Pa), so **no unit conversion is needed anywhere**, only clamps.
- **Inputs of the G36 blocks.** Of the sequence inputs, most map directly to a BOPTEST measurement. Six need a derived signal or a stand-in (section 6): the zone cooling-loop signal `uCoo`, AHU fan status, AHU operation mode, the reheat discharge-air setpoint (neutralized), plant enable (CHW and HW) and the plant staging flag; the unused coil inputs are constants. Zone heating/cooling setpoints do **not** need synthesis: the `/advance` payload carries the setpoint in effect for every `_u` input.
- **Biggest structural gaps.** (1) The HW plant has no AHU hot-water coil, so HW reset requests have to come from the 15 VAV reheat valves and the engine has no Reheat `SystemRequests` or `PlantReset` block vendored. (2) `SupplyFan` does not output its static-pressure setpoint, so use a standalone trim-and-respond (or read an internal connector). (3) The engine's operation-mode integers differ from the numbers hard-coded inside `SupplyTemperature.mo`. (4) The plant dp overwrite upper bound (19.13 MPa) is a placeholder, 40 times the pump head.
- **Control step.** Use 120 s (matches the 120 s sample period of every G36 trim-and-respond and request block); 60 s if you want the 60-second timers to resolve exactly.

## 1. Test case facts used

| Item | Value | Source |
|---|---|---|
| Building | Chicago large office, 3 modeled floors (bottom, middle x10 multiplier, top), 5 zones per floor (core + 4 perimeter), 15 VAV boxes with water reheat, one dual-fan VAV AHU per floor | case doc, `config.json` |
| Plants | CHW: 3 chillers, 3 towers, primary constant-speed pumps, 2 variable secondary pumps. HW: 2 gas boilers, 2 variable pumps. **No HW coil in the AHUs** (only an electric freeze-protection coil) | case doc |
| Occupied hours (HVAC) | 06:00-22:00 weekdays, 06:00-18:00 Saturday | case doc |
| Baseline zone setpoints | occupied heating 20 C (293.15 K), cooling 24 C (297.15 K); unoccupied/setback 15.6 C (288.75 K) and 26.7 C (299.85 K) | case doc, supervisory mode table |
| Baseline AHU | SAT setpoint 12.8 C (285.95 K) in all modes; duct static setpoint 400 Pa; return fan = 0.9 x supply fan; min OA damper 0.3; economizer limits 0 to 15.58 C | case doc |
| Baseline VAV box | flow ratio in [0.3, 1] of design: cooling PI (k = 11, Ti = 60 s) on zone temperature vs cooling setpoint, flow setpoint V_flowSet = ratio x design mass flow / 1.2; heating state holds 0.3; reheat valve PI holds zone at the heating setpoint; damper PI tracks the flow setpoint | `Floor.mo`, `VAVTerminal.mo`, `Control.mo`, `FiveZoneVAV.mo` |
| Baseline plants | CHW supply 5.56 C (278.71 K), CHW dp setpoint 0.5 x 478,250 = 239,125 Pa; HW supply 80 C (353.15 K), HW dp setpoint 0.25 x 478,250 = 119,562.5 Pa; chiller staging wait 15 min, boiler staging wait 30 min (internal, not exposed) | `TestCase.mo`, case doc |
| Default control step | 3600 s (`config.json`), settable with `PUT /step`; the model integrates much finer inside a step, so the step is only the supervisory cadence | `config.json`, `testcase.py` |
| Scenarios | `peak_heat_day` (days 24-38), `typical_heat_day` (215-229), `peak_cool_day` (177-191), `typical_cool_day` (277-291), `mix_day` (18-32); each a two-week test with one-week warm-up under baseline control | case doc |
| KPIs | electric power (AHU fans, freeze coils, CHW pumps, chillers, towers, HW pumps), gas power (boilers), zone temperature and CO2 discomfort for each zone | `kpis.json` |

## 2. Point inventory (summary; full list in `boptest-points.csv`)

Counts (verified against the CSV):

| Group | Measurements | `_u` inputs | `_activate` flags | Names |
|---|---|---|---|---|
| AHU floor 1, 2, 3 | 21 each = 63 | 9 each = 27 | 27 | `floorN_...` |
| Zone / VAV, 5 per floor x 3 floors = 15 | 7 each = 105 | 4 each = 60 | 60 | `floorN_...<Z>...` |
| CHW plant | 7 | 2 | 2 | `reaChiWatSys_*`, `oveChiWatSys_*` |
| HW plant | 6 | 2 | 2 | `reaHotWatSys_*`, `oveHotWatSys_*` |
| Weather | 23 | 0 | 0 | `weaSta_*` |
| **Total** | **204** | **91** | **91** | 386 |

Every `_u` has a sticky partner `<name>_activate` (0 or 1, default 0). Measurements carry no min/max in the documentation. Units are SI throughout (K, Pa, m3/s, W, kg/s, ppm, dimensionless fractions in 0..1).

Name patterns below are expanded by floor N = 1, 2, 3 and zone <Z> = Cor, Sou, Eas, Nor, Wes (BOPTEST `vAV1..vAV5` are in that same order). The pattern expansion reproduces all 386 names exactly (checked by script).

### 2.1 AHU measurements (per floor)

| Point (per floor N = 1..3) | Unit | Description |
|---|---|---|
| `floorN_reaAHU_CO2_AHUFre_y` | ppm | Fresh air CO2 measurement for AHU |
| `floorN_reaAHU_CO2_AHURet_y` | ppm | Return air CO2 measurement for AHU |
| `floorN_reaAHU_CO2_AHUSup_y` | ppm | Supply air CO2 measurement for AHU |
| `floorN_reaAHU_PFanRet_y` | W | Electrical power measurement of return fan for AHU |
| `floorN_reaAHU_PFanSup_y` | W | Electrical power measurement of supply fan for AHU |
| `floorN_reaAHU_PFreCoi_y` | W | Electrical power measurement of freeze protection coil for AHU |
| `floorN_reaAHU_TCooCoiRet_y` | K | Cooling coil return water temperature measurement for AHU |
| `floorN_reaAHU_TCooCoiSup_y` | K | Cooling coil supply water temperature measurement for AHU |
| `floorN_reaAHU_TFreCoiLea_y` | K | Temperature of air leaving freeze protection coil |
| `floorN_reaAHU_TMix_y` | K | Mixed air temperature measurement for AHU |
| `floorN_reaAHU_TRet_y` | K | Return air temperature measurement for AHU |
| `floorN_reaAHU_TSup_y` | K | Supply air temperature measurement for AHU |
| `floorN_reaAHU_V_flowOA_y` | m3/s | Supply outdoor airflow rate measurement for AHU |
| `floorN_reaAHU_V_flowRet_y` | m3/s | Return air flowrate measurement for AHU |
| `floorN_reaAHU_V_flowSup_y` | m3/s | Supply air flowrate measurement for AHU |
| `floorN_reaAHU_dp_y` | Pa | Static pressure of supply fan for AHU |
| `floorN_reaAHU_occ_y` | 1 | Occupancy status (1 occupied, 0 unoccupied) |
| `floorN_reaAHU_phiAHURet_y` | 1 | Return air relative humidity for AHU |
| `floorN_reaAHU_phiAHUSup_y` | 1 | Supply air relative humidity for AHU |
| `floorN_reaAHU_yCooVal_y` | 1 | AHU cooling coil valve position measurement |
| `floorN_reaAHU_yOA_y` | 1 | AHU OA damper position measurement |

### 2.2 AHU overwrite inputs (per floor)

| Overwrite input (per floor N) | Unit | Min .. max | Description |
|---|---|---|---|
| `floorN_TSupAirSet_u` | K | 285.15 .. 313.15 | Supply air temperature setpoint for AHU |
| `floorN_dpSet_u` | Pa | 50 .. 410 | Supply duct pressure setpoint for AHU |
| `floorN_duaFanAirHanUni_cooCoi_yCoo_u` | 1 | 0 .. 1 | Cooling coil valve control signal for AHU |
| `floorN_duaFanAirHanUni_frePro_oveFrePro_u` | 1 | 0 .. 1 | Control signal for freeze protection coil |
| `floorN_duaFanAirHanUni_mixingBox_mixBox_yEA_u` | 1 | 0 .. 1 | Exhaust air damper position setpoint for AHU |
| `floorN_duaFanAirHanUni_mixingBox_mixBox_yOA_u` | 1 | 0 .. 1 | Outside air damper position setpoint for AHU |
| `floorN_duaFanAirHanUni_mixingBox_mixBox_yRet_u` | 1 | 0 .. 1 | Return air damper position setpoint for AHU |
| `floorN_duaFanAirHanUni_oveSpeRetFan_u` | 1 | 0 .. 1 | AHU return fan speed control signal |
| `floorN_duaFanAirHanUni_supFan_oveSpeSupFan_u` | 1 | 0 .. 1 | AHU supply fan speed control signal |

The two setpoint inputs the supervisory loop writes are `floorN_TSupAirSet_u` (K) and `floorN_dpSet_u` (Pa). The other seven are low-level actuator overrides (fan speeds 0..1, damper positions 0..1, coil valve 0..1, freeze coil 0..1). The documentation warns that overwriting any one damper bypasses the interlocks, so the three mixing-box dampers must be driven as a group; the loop does not touch them.

### 2.3 Zone / VAV measurements (per floor and zone)

| Point (per floor N, zone `<Z>` = Cor, Sou, Eas, Nor, Wes) | Unit | Description |
|---|---|---|
| `floorN_reaZon<Z>_CO2Zon_y` | ppm | Zone air CO2 measurement for the zone |
| `floorN_reaZon<Z>_TSup_y` | K | Discharge air temperature measurement for the zone |
| `floorN_reaZon<Z>_TZon_y` | K | Zone air temperature measurement for the zone |
| `floorN_reaZon<Z>_V_flowSet_y` | m3/s | Zone airflow setpoint for the zone |
| `floorN_reaZon<Z>_V_flow_y` | m3/s | Air flowrate measurement for the zone |
| `floorN_reaZon<Z>_yDam_y` | 1 | Damper position measurement for the zone |
| `floorN_reaZon<Z>_yReheaVal_y` | 1 | Reheat valve position measurement for the zone |

### 2.4 Zone / VAV overwrite inputs (per floor and zone)

| Overwrite input (`<k>` = 1..5 for Cor, Sou, Eas, Nor, Wes) | Unit | Min .. max | Description |
|---|---|---|---|
| `floorN_fivZonVAV_vAV<k>_oveZonLoc_yDam_u` | 1 | 0 .. 1 | Damper position signal for the zone |
| `floorN_fivZonVAV_vAV<k>_oveZonLoc_yReaHea_u` | 1 | 0 .. 1 | Reheat control signal for the zone |
| `floorN_oveZon<Z>_TZonCooSet_u` | K | 285.15 .. 313.15 | Zone air temperature cooling setpoint for the zone |
| `floorN_oveZon<Z>_TZonHeaSet_u` | K | 285.15 .. 313.15 | Zone air temperature heating setpoint for the zone |

### 2.5 Chilled-water plant

| Point | Kind | Unit | Min .. max | Description |
|---|---|---|---|---|
| `reaChiWatSys_TCHWRet_y` | meas | K |  | Chilled water return temperature measurement |
| `reaChiWatSys_TCHWSup_y` | meas | K |  | Chilled water supply temperature measurement |
| `reaChiWatSys_dp_y` | meas | Pa |  | Differential pressure of chilled water plant measurement |
| `reaChiWatSys_mCHWTot_y` | meas | kg/s |  | Total chilled water mass flow rate |
| `reaChiWatSys_reaPChi_y` | meas | W |  | Multiple chiller power consumption |
| `reaChiWatSys_reaPCooTow_y` | meas | W |  | Multiple cooling tower power consumption |
| `reaChiWatSys_reaPPum_y` | meas | W |  | Chilled water plant pump power consumption |
| `oveChiWatSys_TWSet_u` | input | K | 278.15 .. 288.15 | Chilled/hot water supply setpoint |
| `oveChiWatSys_dpSet_u` | input | Pa | 0 .. 19130000 | Differential pressure setpoint |

### 2.6 Hot-water plant

| Point | Kind | Unit | Min .. max | Description |
|---|---|---|---|---|
| `reaHotWatSys_THWRet_y` | meas | K |  | Hot water return temperature measurement |
| `reaHotWatSys_THWSup_y` | meas | K |  | Hot water supply temperature measurement |
| `reaHotWatSys_dp_y` | meas | Pa |  | Differential pressure of hot water plant measurement |
| `reaHotWatSys_mHWTot_y` | meas | kg/s |  | Total hot water mass flow rate |
| `reaHotWatSys_reaPBoi_y` | meas | W |  | Multiple gas boiler power consumption |
| `reaHotWatSys_reaPPum_y` | meas | W |  | Hot water plant pump power consumption |
| `oveHotWatSys_TWSet_u` | input | K | 291.15 .. 353.15 | Chilled/hot water supply setpoint |
| `oveHotWatSys_dpSet_u` | input | Pa | 0 .. 19130000 | Differential pressure setpoint |

### 2.7 Weather station

| Point | Unit | Description |
|---|---|---|
| `weaSta_reaWeaCeiHei_y` | m | Cloud cover ceiling height measurement |
| `weaSta_reaWeaCloTim_y` | s | Day number with units of seconds |
| `weaSta_reaWeaHDifHor_y` | W/m2 | Horizontal diffuse solar radiation measurement |
| `weaSta_reaWeaHDirNor_y` | W/m2 | Direct normal radiation measurement |
| `weaSta_reaWeaHGloHor_y` | W/m2 | Global horizontal solar irradiation measurement |
| `weaSta_reaWeaHHorIR_y` | W/m2 | Horizontal infrared irradiation measurement |
| `weaSta_reaWeaLat_y` | rad | Latitude of the location |
| `weaSta_reaWeaLon_y` | rad | Longitude of the location |
| `weaSta_reaWeaNOpa_y` | 1 | Opaque sky cover measurement |
| `weaSta_reaWeaNTot_y` | 1 | Sky cover measurement |
| `weaSta_reaWeaPAtm_y` | Pa | Atmospheric pressure measurement |
| `weaSta_reaWeaRelHum_y` | 1 | Outside relative humidity measurement |
| `weaSta_reaWeaSolAlt_y` | rad | Solar altitude angle measurement |
| `weaSta_reaWeaSolDec_y` | rad | Solar declination angle measurement |
| `weaSta_reaWeaSolHouAng_y` | rad | Solar hour angle measurement |
| `weaSta_reaWeaSolTim_y` | s | Solar time |
| `weaSta_reaWeaSolZen_y` | rad | Solar zenith angle measurement |
| `weaSta_reaWeaTBlaSky_y` | K | Black-body sky temperature measurement |
| `weaSta_reaWeaTDewPoi_y` | K | Dew point temperature measurement |
| `weaSta_reaWeaTDryBul_y` | K | Outside drybulb temperature measurement |
| `weaSta_reaWeaTWetBul_y` | K | Wet bulb temperature measurement |
| `weaSta_reaWeaWinDir_y` | rad | Wind direction measurement |
| `weaSta_reaWeaWinSpe_y` | m/s | Wind speed measurement |

The only weather point the sequences need is `weaSta_reaWeaTDryBul_y` (outdoor dry-bulb, K). BOPTEST exposes no AHU-local outdoor-air temperature, only the mixed-air `reaAHU_TMix_y`.

## 3. BOPTEST behaviors that shape the wiring

1. **Effective inputs come back in the payload.** The full state returned by `/initialize` and `/advance` merges the measurements with every `_u` key; the `_u` value is the signal in effect (the baseline controller's value if the input is not activated, the written value if it is). So the baseline zone setpoints, fan speeds and setpoints are readable without any extra point. Source: `testcase.py` stores the FMU's `<name>_y` readback under the `<name>_u` key.
2. **Unknown keys fail the whole step.** `/advance` returns 400 for any name that is not a real input. Derived signals must never be sent.
3. **Out-of-range writes are truncated, not rejected,** with a warning string in the response message. Clamp in the host.
4. **`_activate` is sticky;** send an explicit 0 to hand control back. Values are 1/0 or true/false.
5. **Timing.** State at the end of a step is returned; a controller reading `y(t)` writes inputs that apply to `[t, t + step]`. The first state (time = start time) comes from `/initialize`, which also runs the warm-up under baseline control.
6. **Zero-order hold.** Inputs written for a step are held for that step; there is no ramping.

## 4. Design constants for the G36 parameters

Zone VAV boxes (from the equipment table in the case doc; min cooling and heating flow are both 0.3 of design, min damper 0.3). `Vdes = design mass flow / 1.2` matches the model's own conversion (`V_flowSet = ratio x m_design / 1.2`), so `V_flowSet_y / Vdes` is the baseline flow ratio. Inferred: the doc's design mass flow is the model's `mAirFloRat`; confirm with the largest `V_flowSet_y` seen in a run. Floor 2 values are ten times the other floors because of the floor multiplier.

| Floor | Zone (BOPTEST vAV index) | Design air, kg/s | Vdes = m/1.2, m3/s | Min cooling/heating flow = 0.3 Vdes, m3/s | Reheat design water, kg/s |
|---|---|---|---|---|---|
| 1 | Cor (vAV1) | 49.1 | 40.92 | 12.28 | 3.88 |
| 1 | Sou (vAV2) | 10.1 | 8.42 | 2.52 | 0.8 |
| 1 | Eas (vAV3) | 6.7 | 5.58 | 1.68 | 0.53 |
| 1 | Nor (vAV4) | 8.6 | 7.17 | 2.15 | 0.68 |
| 1 | Wes (vAV5) | 7.8 | 6.50 | 1.95 | 0.62 |
| 2 | Cor (vAV1) | 491.4 | 409.50 | 122.85 | 38.8 |
| 2 | Sou (vAV2) | 101.2 | 84.33 | 25.30 | 8 |
| 2 | Eas (vAV3) | 67 | 55.83 | 16.75 | 5.3 |
| 2 | Nor (vAV4) | 85.5 | 71.25 | 21.38 | 6.8 |
| 2 | Wes (vAV5) | 77.9 | 64.92 | 19.48 | 6.2 |
| 3 | Cor (vAV1) | 49.1 | 40.92 | 12.28 | 3.88 |
| 3 | Sou (vAV2) | 10.1 | 8.42 | 2.52 | 0.8 |
| 3 | Eas (vAV3) | 6.7 | 5.58 | 1.68 | 0.53 |
| 3 | Nor (vAV4) | 8.6 | 7.17 | 2.15 | 0.68 |
| 3 | Wes (vAV5) | 7.8 | 6.50 | 1.95 | 0.62 |

AHUs (case doc):

| Floor | Cooling coil: capacity W, CHW design flow kg/s, design airflow m3/s | Supply fan: design kg/s, head Pa, design power W | Return fan: head Pa, design power W | Freeze coil W |
|---|---|---|---|---|
| 1 (bottom) | 750,000 / 26 / 70 | 70 / 1000 / 97,222 | 300 / 29,167 | 1,650,000 |
| 2 (middle, x10) | 7,500,000 / 260 / 700 | 700 / 1000 / 972,222 | 300 / 291,667 | 16,500,000 |
| 3 (top) | 1,000,000 / 30 / 70 | 70 / 1000 / 97,222 | 300 / 29,167 | 1,650,000 |

Plants: 3 chillers at 2.6 MW, CHW secondary pumps 155 kg/s at 478,000 Pa, CHW primary 105 kg/s at 211,000 Pa; 2 boilers at 3.3 MW, HW pumps 39 kg/s at 478,250 Pa. Baseline setpoints are listed in section 1.

## 5. Wiring of each G36 subsequence

Conventions. "Effective `_u`" means the value of that `_u` key in the latest `/advance` or `/initialize` payload. "SYNTHESIZE" refers to the recipes in section 6. Instance counts assume one instance per zone (15) or per AHU (3). Parameter values in the engine fixtures are G36 defaults unless I say otherwise. Block and fixture names are the engine's: `crates/oce-cxf/tests/fixtures/g36/*.jsonld`.

### 5.1 `Generic.TimeSuppression` (15 instances, one per zone; fixture `generic_time_suppression`)

Purpose: after a zone setpoint change, suppress temperature requests for a time that grows with the size of the change (G36 5.1.20). Output feeds `SystemRequests.uAftSup`.

| Input | Type, unit | BOPTEST source |
|---|---|---|
| `TSet` | Real, K | effective `floorN_oveZon<Z>_TZonCooSet_u` (baseline 297.15 K occupied, 299.85 K unoccupied) |
| `TZon` | Real, K | `floorN_reaZon<Z>_TZon_y` |

| Parameter | Value | Note |
|---|---|---|
| `chaRat` | 540 s/K | G36 default |
| `maxTim` | 1800 s | G36 default |
| `samplePeriod` | 120 s | equal to the control step |
| `dTHys` | 0.25 K | G36 default |

Output `yAftSup` (Boolean) goes to 5.2. In this case the cooling setpoint changes only at occupancy transitions (24 C to 26.7 C and back), so the suppression is active around 06:00 and 22:00.

### 5.2 `TerminalUnits.CoolingOnly.Subsequences.SystemRequests` (15 instances; fixture `cooling_only_system_requests`)

G36 logic, paraphrased from the vendored documentation: temperature requests are 3 when the zone is more than 3 K above its cooling setpoint for 2 min (after suppression), 2 when more than 2 K for 2 min, 1 when the cooling loop is above 95% until it drops under 85%, else 0. Static-pressure requests are 3 when airflow is under 50% of its setpoint with the damper above 95% for 1 min, 2 when under 70% in the same condition, 1 when the damper is above 95% until it drops under 85%, else 0. The block is named "cooling only" but this is the same logic a reheat box uses for these two requests; it has no hot-water request. See 5.3.

| Input | Type, unit | BOPTEST source |
|---|---|---|
| `uAftSup` | Boolean | `TimeSuppression.yAftSup` (5.1) |
| `TCooSet` | Real, K | effective `floorN_oveZon<Z>_TZonCooSet_u` |
| `TZon` | Real, K | `floorN_reaZon<Z>_TZon_y` |
| `uCoo` | Real, 1 | **SYNTHESIZE** (6.1) |
| `VSet_flow` | Real, m3/s | `floorN_reaZon<Z>_V_flowSet_y` |
| `VDis_flow` | Real, m3/s | `floorN_reaZon<Z>_V_flow_y` |
| `uDam` | Real, 1 | `floorN_reaZon<Z>_yDam_y` |

| Parameter | Value | Note |
|---|---|---|
| `thrTemDif`, `twoTemDif` | 3 K, 2 K | G36 |
| `durTimTem`, `durTimFlo` | 120 s, 60 s | the 60 s timer resolves to 120 s at a 120 s step (section 8) |
| `dTHys`, `floHys`, `looHys`, `damPosHys` | 0.25 K, 0.01 m3/s, 0.01, 0.01 | fixture defaults |
| `samplePeriod` | 120 s | |

Outputs: `yZonTemResReq` and `yZonPreResReq` (Integer, 0..3). Sum the five values per AHU with an integer multi-sum: `ZonTemResReq` feeds 5.4, `ZonPreResReq` feeds 5.5.

### 5.3 Reheat hot-water requests (15 instances, from the VAV reheat valves; fixture `multizone_vav_plant_requests`, reused)

The HW plant serves only the VAV reheat coils, and the engine has no Reheat `SystemRequests`. The reusable piece is the vendored AHU `PlantRequests` block, whose hot-water branch is "valve above 95% sends 1 request until under 85%; plant request while above 95% until under 10%". Use one instance per zone with the following wiring and ignore its chilled-water outputs:

| Input | Type, unit | BOPTEST source |
|---|---|---|
| `TAirSup` | Real, K | `floorN_reaZon<Z>_TSup_y` |
| `TAirSupSet` | Real, K | **SYNTHESIZE** (6.4): wire the same signal as `TAirSup`, so the two deviation branches (17 K and 8 K below setpoint for 5 min) never fire |
| `uCooCoiSet` | Real, 1 | constant 0 |
| `uHeaCoiSet` | Real, 1 | `floorN_reaZon<Z>_yReheaVal_y` |

Parameters: fixture defaults (`heaCoi` and `cooCoi` water-based, `THys` 0.1 K, `posHys` 0.05). Outputs used: `yHotWatResReq` (0 or 1 with the deviation branches neutralized), `yHotWatPlaReq` (0/1). Sum `yHotWatResReq` over the 15 zones for the HW plant (0..15).

Alternative that avoids the oddity of feeding one signal twice: build the request count from palette blocks (valve-above-threshold with a short true-delay, summed), as `plant-reset.md` suggests. Either way this is new authored wiring, not a vendored G36 sequence. This is a deliberate simplification of G36's discharge-air-deviation requests, and it is the second gap to mention in the demo notes.

### 5.4 `AHUs.MultiZone.VAV.SetPoints.SupplyTemperature` (3 instances; fixture `multizone_vav_supply_temperature`)

Logic (vendored documentation): in the occupied modes the setpoint falls linearly from the "maximum supply temperature" (itself reset by trim-and-respond between `TSupCoo_min` and `TSupCoo_max`) when outdoor air is at or below `TOut_min`, down to `TSupCoo_min` when outdoor air is at or above `TOut_max`. Cool-down mode returns `TSupCoo_min`; warm-up and setback return `TSupWarUpSetBac`; with the fan off it returns `TDeaBan`.

| Input | Type, unit | BOPTEST source |
|---|---|---|
| `TOut` | Real, K | `weaSta_reaWeaTDryBul_y` |
| `u1SupFan` | Boolean | **SYNTHESIZE** (6.2) |
| `uOpeMod` | Integer | **SYNTHESIZE** (6.3) |
| `uZonTemResReq` | Integer | sum of the five zones' `yZonTemResReq` on this floor (5.2) |

| Parameter | Value | Note |
|---|---|---|
| `TSupCoo_min` | 285.95 K (12.8 C) | **proposal**: the baseline SAT. G36 advises not going below the design coil leaving temperature, which I take to be the baseline 12.8 C (inferred; the case doc gives no coil leaving-air design value). The fixture default is 285.15 K, which is also BOPTEST's lower bound |
| `TSupCoo_max` | 291.15 K (18 C) | fixture default and G36 typical ceiling |
| `TOut_min`, `TOut_max` | 289.15 K, 294.15 K | fixture defaults (16 C, 21 C) |
| `delTim`, `samplePeriod` | 600 s, 120 s | |
| `numIgnReq` | 2 | |
| `triAmo`, `resAmo`, `maxRes` | +0.1 K, -0.2 K, -0.6 K | |
| `iniSet`, `maxSet`, `minSet` | `TSupCoo_max`, `TSupCoo_max`, `TSupCoo_min` | fixture bindings |
| `TSupWarUpSetBac`, `TDeaBan` | 308.15 K, 299.15 K | not written to BOPTEST (see below) |

Output `TAirSupSet` (K) goes to `floorN_TSupAirSet_u` (7). **Gate the write:** activate the overwrite only while the AHU mode is occupied; relinquish otherwise, so BOPTEST's own 12.8 C setpoint applies in setback/setup/unoccupied. The warm-up/setback branch would command 35 C supply air, which this AHU cannot make (no heating coil besides freeze protection). The internal trim-and-respond instance is `maxSupTemRes`; its output `y` is the maximum-supply-temperature loop, useful as a diagnostic.

### 5.5 Duct static pressure reset (3 instances)

`AHUs.MultiZone.VAV.SetPoints.SupplyFan` (fixture `multizone_vav_supply_fan`) contains the static-pressure trim-and-respond (instance `staPreSetRes`) but does **not** output its setpoint: its outputs are the fan proof `y1SupFan` and a fan speed `ySupFan` from a PI against `dpDuc`. BOPTEST runs its own fan PI against `floorN_dpSet_u`, so the speed output is unused. Two ways:

- **Recommended (a): standalone `Generic.TrimAndRespond`** (fixture `trim_and_respond_have_hol_false`), one per AHU:

| Input | Type | BOPTEST source |
|---|---|---|
| `numOfReq` | Integer | sum of the five zones' `yZonPreResReq` (5.2) |
| `uDevSta` | Boolean | **SYNTHESIZE** fan status (6.2) |

| Parameter | Value | Note |
|---|---|---|
| `iniSet` | 120 Pa | G36 `SupplyFan` default; raise if the morning pull-down shows unmet zones |
| `minSet` | 50 Pa | BOPTEST lower bound (G36 default 25 Pa is below it) |
| `maxSet` | 400 Pa | the baseline duct setpoint; the max of the reset reproduces the baseline. The G36 fixture has 410 Pa, which is also the BOPTEST upper bound |
| `delTim`, `samplePeriod` | 600 s, 120 s | |
| `numIgnReq` | 2 | |
| `triAmo`, `resAmo`, `maxRes` | -12 Pa, +15 Pa, +32 Pa | `SupplyFan` defaults |
| `have_hol` | false | as in all vendored fixtures |

- **(b): use `SupplyFan` unchanged** (inputs: `uOpeMod` from 6.3, `dpDuc` = `floorN_reaAHU_dp_y` in Pa, `uZonPreResReq` = the per-AHU sum) and read the internal connector `staPreSetRes.y` with the engine's `get_output`/`watch`. This keeps the fixture byte-for-byte but depends on addressing a nested connector; I have not run it.

Output (Pa) goes to `floorN_dpSet_u`, clamped to 50..410, and is gated with the same occupied-only rule as 5.4.

### 5.6 AHU `PlantRequests` (3 instances; fixture `multizone_vav_plant_requests`)

Logic (vendored documentation): chilled-water reset requests are 3 when supply air is more than 3 K above its setpoint for 2 min, 2 when more than 2 K for 2 min, 1 when the cooling valve is above 95% until it drops under 85%, else 0. The chiller plant request is 1 while the valve is above 95% until it drops under 10%.

| Input | Type, unit | BOPTEST source |
|---|---|---|
| `TAirSup` | Real, K | `floorN_reaAHU_TSup_y` |
| `TAirSupSet` | Real, K | the 5.4 output `TAirSupSet` (or the effective `floorN_TSupAirSet_u`, which is the same value while overwritten and the baseline 285.95 K otherwise) |
| `uCooCoiSet` | Real, 1 | `floorN_reaAHU_yCooVal_y` (a measured valve position stands in for the commanded one) |
| `uHeaCoiSet` | Real, 1 | constant 0 (no HW coil) |

Parameters: fixture defaults (`cooCoi` water-based, `THys` 0.1 K, `posHys` 0.05). Outputs used: `yChiWatResReq` (0..3) and `yChiPlaReq` (0/1). Ignore the hot-water outputs. The fixture keeps `heaCoi` water-based, so no structural change is needed; the hot-water outputs are simply not used.

### 5.7 Plant resets (CHW and HW)

The reset block is `Buildings.Templates.Plants.Controls.Setpoints.PlantReset`, which is **not** in the engine's vendored set; `plant-reset.md` covers porting it (estimated 3 to 5 hours for a loading CHW instance) and proposes the parameter values. Wiring:

| Input of PlantReset (CHW / HW) | Type | Source |
|---|---|---|
| request count (`nReqRes`) | Integer | CHW: sum of the three AHUs' `yChiWatResReq` (0..9). HW: sum of the 15 reheat `yHotWatResReq` (5.3) |
| enable (`u1Ena`) | Boolean | **SYNTHESIZE** (6.5) |
| staging in progress (`u1StaPro`) | Boolean | **SYNTHESIZE**: constant false in v1 (6.5) |

| Output | Unit | BOPTEST input | Clamp |
|---|---|---|---|
| CHW `TSupSet` | K | `oveChiWatSys_TWSet_u` | 278.15 .. 288.15 |
| CHW `dpSet[1]` | Pa | `oveChiWatSys_dpSet_u` | 0 .. 478,250 (design guard; BOPTEST's own upper bound is 19,130,000) |
| HW `TSupSet` | K | `oveHotWatSys_TWSet_u` | 291.15 .. 353.15 |
| HW `dpSet[1]` | Pa | `oveHotWatSys_dpSet_u` | 0 .. 478,250 |

Values proposed in `plant-reset.md` section 4.2 (reproduced so this file stands alone; they are that file's proposals, not mine): CHW design supply 278.71 K and relaxed limit 282.15 K, dp from 239,125 Pa down to 95,650 Pa; HW design 353.15 K and limit 343.15 K, dp from 119,562.5 Pa down to 47,825 Pa; trim -0.02, respond +0.03 per excess request (cap 0.07), ignore 2 requests, delay 900 s, sample period 300 s, hold 900 s. Note that the PlantReset sample period (300 s) is not a multiple of the 120 s control step: use a 60 s or 300 s step for an exact grid, or accept that its sampling lands on the first tick at or after each 300 s boundary (inferred from how sample timers work on a tick grid; check on the first run).

**Stand-in if PlantReset is not ported: two `Generic.TrimAndRespond` per loop** (this is in the vendored set). I derived these amounts from the PlantReset values: the temperature rows reproduce its temperature slope (reset value r from 1 to 0.5 over 25 samples) and the dp rows its dp slope (r from 0.5 to 0). Run as separate loops, dp starts relaxing at the same time as temperature instead of after it, so the "temperature first, then pressure" coupling is lost:

| Loop and variable | iniSet | minSet | maxSet | triAmo | resAmo | maxRes | delTim / samplePeriod |
|---|---|---|---|---|---|---|---|
| CHW supply temperature (K) | 278.71 | 278.71 | 282.15 | +0.14 | -0.21 | -0.48 | 900 s / 300 s |
| CHW dp (Pa) | 239,125 | 95,650 | 239,125 | -5,739 | +8,609 | +20,087 | 900 s / 300 s |
| HW supply temperature (K) | 353.15 | 343.15 | 353.15 | -0.40 | +0.60 | +1.40 | 900 s / 300 s |
| HW dp (Pa) | 119,562.5 | 47,825 | 119,562.5 | -2,870 | +4,304 | +10,043 | 900 s / 300 s |

`numIgnReq` = 2; `numOfReq` and `uDevSta` as in the table above. For the CHW temperature the sign convention is that the T&R output rises (relaxes) when requests are at or under the ignore count and falls when above, the same as the AHU supply-temperature loop; for HW the output falls when relaxing, so trim is negative.

### 5.8 Generic `TrimAndRespond` parameter summary

| Use | Block | numOfReq source | uDevSta source | iniSet / minSet / maxSet | triAmo / resAmo / maxRes | delTim / sample |
|---|---|---|---|---|---|---|
| AHU SAT (inside 5.4) | `maxSupTemRes` | per-AHU sum of `yZonTemResReq` | fan status | 291.15 / 285.95 / 291.15 K | +0.1 / -0.2 / -0.6 K | 600 / 120 s |
| AHU duct static (5.5) | standalone or `staPreSetRes` | per-AHU sum of `yZonPreResReq` | fan status | 120 / 50 / 400 Pa | -12 / +15 / +32 Pa | 600 / 120 s |
| CHW, HW plants (5.7) | inside PlantReset, or the stand-in above | sums of AHU / reheat requests | plant enable | see 5.7 | see 5.7 | 900 / 300 s |

## 6. Derived (synthesized) signals

### 6.1 Zone cooling-loop signal `uCoo` (15 signals)

Two options; (a) is my default.

- **(a) From the baseline flow ratio.** In cooling state BOPTEST maps the cooling loop to airflow between 0.3 and 1.0 of design; in deadband and heating it holds 0.3. So `uCoo = clamp((V_flowSet_y / Vdes - 0.3) / 0.7, 0, 1)` using `floorN_reaZon<Z>_V_flowSet_y` and the `Vdes` column of section 4. This follows what BOPTEST's VAV is actually doing. Caveat: the baseline cooling PI is stiff (k = 11 per kelvin), so `uCoo` reaches 1 with a zone only a few tenths of a kelvin above setpoint and the "above 95%" rule produces 1 request often. The 3-request and 2-request rules (temperature based) are unaffected.
- **(b) G36 loop.** Feed the engine's `ThermalZones.ControlLoops` block (fixture `thermal_zones_control_loops`, `kCooCon` 0.1, `TiCooCon` 900 s, `kHeaCon` 0.1, `TiHeaCon` 900 s) with `TZon` = `..._TZon_y`, `TCooSet` and `THeaSet` = effective `_u` setpoints, and use its `yCoo` output. Slower and closer to G36, but it is a parallel model that BOPTEST's boxes do not follow.

### 6.2 AHU fan status (`u1SupFan`, T&R `uDevSta`; 3 signals)

No fan-status measurement exists. Use `floorN_reaAHU_V_flowSup_y` against about 5% of the design airflow (3.5 m3/s on floors 1 and 3, 35 m3/s on floor 2), on at 5% and off at 3% (Hysteresis block). Alternative: supply-fan power `floorN_reaAHU_PFanSup_y` against 1% of design power (97,222 W, 972,222 W); weaker, because fan power falls with the cube of speed. Whichever is chosen, expose it as the derived BACnet point `floorN_g36_SupFanSta`.

### 6.3 AHU operation mode (`uOpeMod`; 3 signals)

BOPTEST has no G36 mode. Use the engine's numbering (occupied 1, coolDown 2, setUp 3, warmUp 4, setBack 5, freezeProtection 6, unoccupied 7): `floorN_reaAHU_occ_y` = 1 gives 1 (occupied); otherwise 7 (unoccupied). Setback (5) and setup (3) can be told apart while the fan runs unoccupied (min zone temperature low or max zone temperature high, in BOPTEST's night-cycle logic), but nothing in the sequences needs them because the writes are gated to occupied. Warm-up and cool-down do not exist in the baseline, so they are not produced.

### 6.4 Reheat discharge-air setpoint (15 signals)

BOPTEST has no discharge-air setpoint for the reheat boxes (its reheat PI works on zone temperature). Do not invent one; neutralize the deviation branches as in 5.3 (same signal into `TAirSup` and `TAirSupSet`).

### 6.5 Plant enable and staging

- CHW enable: occupied on any floor, or any AHU `yChiPlaReq` = 1. Flow alternative: `reaChiWatSys_mCHWTot_y` above about 2% of the secondary design flow (155 kg/s, about 3 kg/s) with hysteresis. HW enable: occupied on any floor or any reheat plant request, or `reaHotWatSys_mHWTot_y` above about 0.8 kg/s (2% of 39 kg/s). Per `plant-reset.md` (from `TestCase.mo`), the baseline switches the chiller side off when outdoor dry-bulb is under 283.15 K and unoccupied, so enable-by-occupancy is close to what the plant does.
- Staging flag: not observable (staging is internal; chillers wait 15 min, boilers 30 min). v1 constant false. v2 could infer from a step in chiller or boiler power.

## 7. Sequence output to BOPTEST input map

| # | Sequence output | Unit | BOPTEST `_u` | Range for clamp | Conversion | Gate |
|---|---|---|---|---|---|---|
| 1-3 | `SupplyTemperature.TAirSupSet` floor N | K | `floorN_TSupAirSet_u` | 285.15 .. 313.15 | none | occupied only |
| 4-6 | static-pressure T&R `y` floor N | Pa | `floorN_dpSet_u` | 50 .. 410 | none | occupied only |
| 7 | CHW `TSupSet` | K | `oveChiWatSys_TWSet_u` | 278.15 .. 288.15 | none | plant enabled |
| 8 | CHW `dpSet` | Pa | `oveChiWatSys_dpSet_u` | 0 .. 478,250 (design guard) | none | plant enabled |
| 9 | HW `TSupSet` | K | `oveHotWatSys_TWSet_u` | 291.15 .. 353.15 | none | plant enabled |
| 10 | HW `dpSet` | Pa | `oveHotWatSys_dpSet_u` | 0 .. 478,250 (design guard) | none | plant enabled |

Each write also sets `<name>_activate` = 1; outside its gate the host relinquishes the BACnet priority slot and the next step sends activate = 0. When the plant is disabled the baseline plant logic is also doing its own thing, so releasing is the safe choice.

The engine side uses the engine's `HostTick`: one `execute_frame` per tick, non-decreasing seconds, every input present. All real values are SI and are passed as they come from BOPTEST.

## 8. Tick order and timing

Per control step (proposal, 120 s):

1. Take the measurements and effective `_u` values from the last `/advance` (or `/initialize`) payload; update the derived signals (section 6).
2. Call the engine once with `t` = the payload `time` (seconds, absolute). Stateful blocks (trim-and-respond, timers) use this time, so start the loop at the scenario start, which is a multiple of 3600 s, to keep the 120 s and 300 s sample grids aligned.
3. Read the 10 outputs plus the diagnostics, clamp, apply gates, write the BACnet AOs (`bacnet-device-table.md`, section 4).
4. Build the `_u`/`_activate` payload from the priority arrays and POST `/advance`. The response is the next tick's input.

Timer resolution at a 120 s step: the engine evaluates timers on ticks, so a 60 s delay (static-pressure request) fires on the next tick, effectively 120 s; the 5 min HW deviation timers (disabled here) and 2 min timers align; the PlantReset 300 s grid does not divide by 120 s (section 5.7). A 60 s step doubles the run cost and resolves all of them; 300 s makes 2-min and 1-min timers coarser. A two-week scenario is 10,080 steps at 120 s (20,160 at 60 s); the one-week warm-up runs inside `/initialize`.

## 9. Gaps and risks (ranked)

1. **HW request path is authored, not vendored.** No AHU hot-water coil; the engine has no Reheat `SystemRequests` and no Plants package. The reheat requests in 5.3 are a simplification of G36 and need a note in the demo.
2. **PlantReset is outside the vendored set.** Port required (see `plant-reset.md`), or use the stand-in T&R pairs with the coupling lost.
3. **`SupplyFan` hides its dp setpoint.** Standalone T&R is the safe route (5.5).
4. **Operation-mode numbering mismatch.** The engine maps `coolDown` = 2, `setUp` = 3 (`oce-model/src/types.rs`), while `SupplyTemperature.mo` hard-codes cool-down as 3 and treats 1..2 as occupied, 4..5 as warm-up/setback. With only mode 1 (occupied) driving writes this is harmless today, but it will bite if modes 2 to 5 are produced. I could not fetch the upstream `OperationModes` type to settle which numbering is canonical (the fetch failed).
5. **`uCoo` is synthesized,** and the baseline PI is stiff; expect more "1 request" states than a G36 loop would give. Option (b) in 6.1 is the cross-check.
6. **Plant dp upper bound is a placeholder** (19.13 MPa = 40 x pump head). A bad sequence could write a very large value that BOPTEST accepts. Clamp in the host (section 7).
7. **Floor 2 is ten times the load** but counts once in every request sum; equal weighting is the G36 convention, but plant behavior will follow floor 2.
8. **Plant enable and staging are inferred.** No chiller or boiler status points exist.
9. **Overriding SAT and duct static changes the baseline's coupling.** Lower dp can starve zones while BOPTEST's damper PI saturates; this is the intended trade-off, and the unmet-discomfort KPI is where it shows.
10. **Parameter overrides at run time.** The engine exposes `set_param`/`get_param` with a halt/set/resume lifecycle (`oce-api/src/params.rs`), good for numeric T&R parameters. Structural parameters such as the coil-type enumerations change the block's interface, so use separate fixture variants for those instead of `set_param` (inferred, not tested).
11. **Timer grid.** See section 8.
12. **Not run.** No BOPTEST container and no engine run were available here; all behavior is read from source.

## 10. First-run checks

1. After `/initialize`, confirm the payload contains `floor1_TSupAirSet_u` (expect about 285.95 K) and `floor1_dpSet_u` (expect 400 Pa), and the zone setpoint `_u` values (expect 293.15 K and 297.15 K occupied).
2. Compare the largest `V_flowSet_y` per zone with the `Vdes` table; if they differ, fix the table (6.1).
3. Write one setpoint with activate = 1, advance, and confirm the same `_u` is echoed in the payload; then release (activate = 0) and confirm the baseline value returns.
4. Confirm that an out-of-range `_u` is truncated with a warning (so the host clamp is the only protection).
5. Run one unoccupied night and confirm the writes are released and the baseline setpoints return.
6. Check that the occupancy transition at 06:00 starts the trim-and-respond delay (600 s) and that the first SAT output stays at 291.15 K until requests appear.
7. Run one summer and one winter scenario and compare KPIs against a baseline-only run: electric power, thermal discomfort, CO2.
