# BACnet device table: BOPTEST `multizone_office_complex_air`

Project Mechanical Muse, 2026-10-26. A design proposal; the mirror built so far is the smaller read-only one in `docs/bacnet-mirror.md`.
Sources: BOPTEST `bacnet/BopTestProxy.py`, `bacnet/README.md`, `testcases/multizone_office_complex_air/models/bacnet.ttl`, `testcase.py`; Rusty BACnet `docs/python-api.md`, `website/.../reference/python/server.md`, `crates/rusty-bacnet/rusty_bacnet.pyi`, `crates/bacnet-types/src/enums/units.rs`.
Companions: `boptest-point-map.md` (G36 wiring) and `boptest-points.csv` (every BOPTEST point, with units and ranges). The full 414-row object table (`bacnet-objects.csv` below) was generated during research and is not committed. Everything below is a proposal unless it says "stock proxy" or "BOPTEST".

## 1. Headline

- Proposed layout: **22 BACnet devices, 414 objects**. 3 AHU devices, 15 VAV-box devices (one per zone), 1 CHW plant, 1 HW plant, 1 weather station, 1 gateway.
- Object mix: 296 AI, 23 BI, 3 MSI, 92 AO. No AV, BV or MSV objects, on purpose (section 5).
- 295 objects mirror BOPTEST data points one to one: 204 measurements (201 AI, 3 BI for the `occ_y` flags) and 91 `_u` overwrite inputs (AO). `_activate` flags are **not** objects; they are derived from the AO priority array (section 4).
- 117 objects are host-derived G36 signals (zone requests, AHU/plant request sums, operation mode, fan status). They are read-only to BACnet clients and updated by the Rust host. Plus 2 gateway objects (`time`, `advance`).
- Only **10 of the 91 AOs** are written by the supervisory loop: `floorN_TSupAirSet_u`, `floorN_dpSet_u` (N = 1..3) and the four plant setpoints. The other 81 AOs exist for full parity with the stock proxy and for operator override.
- Stock proxy comparison: one device (instance 599), 297 objects, one global instance counter, unitless. This proposal keeps `599`, `time` and `advance` as the gateway so existing clients that only use those two points keep working, and gives every other point a device that matches a piece of equipment.

## 2. What the stock proxy actually does (facts from the source)

1. **One device, global counter.** `bacnet.ttl` declares a single device `boptest-proxy-device` with instance 599. All 297 objects (92 AO, 1 AV, 204 AI) get instance numbers 1..297 from one running counter across object types, in FMU variable order (not alphabetical). `advance` is `analog-output,1`, `time` is `analog-value,2`, the first measurement is instance 3.
2. **Only three object types.** The class map covers analog-input, analog-output and analog-value. Every measurement, including the `occ_y` occupancy flag (0/1), is an AI. Every `_u` input is an AO.
3. **No units in this test case.** The proxy can attach units only for K and ppm through an internal map, and it reads them from `brick:hasUnit` entries in the ttl. The ttl for this case contains no unit entries at all, so every legacy object is unitless. This proposal adds units (section 3.2).
4. **Initial values.** Objects start from the `/initialize` response; AOs also take that value as their relinquish default. Status flags come from the ttl (all 297 are 0 here).
5. **How a write becomes an overwrite.** On every refresh the proxy first sets `<name>_activate = 0` for all 91 inputs. Then, for each input object whose priority array has any non-null slot, it sends `<name>_u` = the highest-priority value and `<name>_activate = 1`. The priority level does not matter; a plain write to Present_Value lands at priority 16 in BACpypes and counts the same. Relinquishing the slot makes the next refresh send activate = 0, and BOPTEST returns to its baseline controller.
6. **Result lag of one refresh.** The response of `/advance` is stored and only shown on the objects at the next refresh. Clients therefore see measurements one control step late. A host that feeds the controller and the BACnet objects from the same response avoids this.
7. **Two stepping modes.** Periodic (`--app_interval` seconds, default 5) or `oncommand`: the client increments the `advance` AO and the proxy advances once per increment above its internal counter. `--control_step` sets the BOPTEST step through `PUT /step` (BOPTEST's own default is 3600 s).
8. **Quirks worth knowing.** The `advance` registration uses a substring test (`name in 'advance'`), which is harmless for these names. The proxy does not check that an `_activate` partner exists for an input. A commandable-AI class is defined in the file but never used.

BOPTEST facts that matter for any BACnet front end (from `testcase.py`):

- `/advance` accepts only names that are real BOPTEST inputs (`..._u`, `..._activate`); an unknown key returns HTTP 400 and no step happens. Host-derived names (anything with `g36`) must never be sent.
- `_activate` must be 0 or 1. `_u` is a float; out-of-range values are **truncated to the min/max with a warning, not rejected**.
- `_activate` is sticky inside BOPTEST. The host must therefore send an explicit 0 when a point is released.
- The `/advance` payload returns the full current state: all measurements and also the `_u` values (these are the values in effect, the baseline controller's output when the point is not overwritten). This is what lets the BACnet side show the live baseline setpoints, including zone heating/cooling setpoints.

## 3. Proposed topology

### 3.1 Devices and instance numbers

Device instance scheme (all numbers are proposals):

- `599` gateway (kept from the stock proxy), `1000` weather, `1001` CHW plant, `1002` HW plant.
- AHUs: `2000 + floor` (2001, 2002, 2003).
- VAV boxes: `3000 + 10*floor + zone`, zone 1..5 = Core, South, East, North, West. This is the same order as BOPTEST `vAV1..vAV5` (`vAV1`=cor, `vAV2`=sou, `vAV3`=eas, `vAV4`=nor, `vAV5`=wes), checked against all 30 `_u` rows. Examples: 3011 = floor 1 core, 3025 = floor 2 west, 3033 = floor 3 east.
- Object instances are numbered **per device and per object type, starting at 1**, in `bacnet.ttl` order for mirrored points and then the derived points. BACnet instance spaces are per type, so the stock proxy's global counter is not needed. The `legacy_ttl_instance` column in `bacnet-objects.csv` gives the old instance for every mirrored point.
- Object names equal the BOPTEST point names exactly for mirrored points (so a client can paste the name into a BOPTEST call). Derived points use the prefix `floorN_g36_<Zone>_...` or `g36_...`.
- All AHU devices have identical object shapes (37 objects each), and all 15 VAV devices have identical shapes (17 objects each), checked programmatically; only the floor/zone tokens differ.

| Device instance | Device name | Represents | Objects | Object mix |
|---|---|---|---|---|
| 599 | BOPTEST-GW | Gateway / clock: `time`, `advance` (proxy parity) | 2 | 1 AI, 1 AO |
| 1000 | WEATHER | Weather station | 23 | 23 AI |
| 1001 | CHW-PLANT | Chilled-water plant (3 chillers, 3 towers, secondary CHW pumps) | 12 | 9 AI, 1 BI, 2 AO |
| 1002 | HW-PLANT | Hot-water plant (2 boilers, secondary HW pumps) | 11 | 8 AI, 1 BI, 2 AO |
| 2001 | AHU-1 | Floor 1 dual-fan VAV AHU (bottom floor) | 37 | 25 AI, 2 BI, 1 MSI, 9 AO |
| 2002 | AHU-2 | Floor 2 dual-fan VAV AHU (middle x10 floor) | 37 | 25 AI, 2 BI, 1 MSI, 9 AO |
| 2003 | AHU-3 | Floor 3 dual-fan VAV AHU (top floor) | 37 | 25 AI, 2 BI, 1 MSI, 9 AO |
| 3011 | VAV-1-COR | Floor 1 cor zone VAV box + reheat (BOPTEST `vAV1`) | 17 | 12 AI, 1 BI, 4 AO |
| 3012 | VAV-1-SOU | Floor 1 sou zone VAV box + reheat (BOPTEST `vAV2`) | 17 | 12 AI, 1 BI, 4 AO |
| 3013 | VAV-1-EAS | Floor 1 eas zone VAV box + reheat (BOPTEST `vAV3`) | 17 | 12 AI, 1 BI, 4 AO |
| 3014 | VAV-1-NOR | Floor 1 nor zone VAV box + reheat (BOPTEST `vAV4`) | 17 | 12 AI, 1 BI, 4 AO |
| 3015 | VAV-1-WES | Floor 1 wes zone VAV box + reheat (BOPTEST `vAV5`) | 17 | 12 AI, 1 BI, 4 AO |
| 3021 | VAV-2-COR | Floor 2 cor zone VAV box + reheat (BOPTEST `vAV1`) | 17 | 12 AI, 1 BI, 4 AO |
| 3022 | VAV-2-SOU | Floor 2 sou zone VAV box + reheat (BOPTEST `vAV2`) | 17 | 12 AI, 1 BI, 4 AO |
| 3023 | VAV-2-EAS | Floor 2 eas zone VAV box + reheat (BOPTEST `vAV3`) | 17 | 12 AI, 1 BI, 4 AO |
| 3024 | VAV-2-NOR | Floor 2 nor zone VAV box + reheat (BOPTEST `vAV4`) | 17 | 12 AI, 1 BI, 4 AO |
| 3025 | VAV-2-WES | Floor 2 wes zone VAV box + reheat (BOPTEST `vAV5`) | 17 | 12 AI, 1 BI, 4 AO |
| 3031 | VAV-3-COR | Floor 3 cor zone VAV box + reheat (BOPTEST `vAV1`) | 17 | 12 AI, 1 BI, 4 AO |
| 3032 | VAV-3-SOU | Floor 3 sou zone VAV box + reheat (BOPTEST `vAV2`) | 17 | 12 AI, 1 BI, 4 AO |
| 3033 | VAV-3-EAS | Floor 3 eas zone VAV box + reheat (BOPTEST `vAV3`) | 17 | 12 AI, 1 BI, 4 AO |
| 3034 | VAV-3-NOR | Floor 3 nor zone VAV box + reheat (BOPTEST `vAV4`) | 17 | 12 AI, 1 BI, 4 AO |
| 3035 | VAV-3-WES | Floor 3 wes zone VAV box + reheat (BOPTEST `vAV5`) | 17 | 12 AI, 1 BI, 4 AO |

Floor 2 is the 10x floor: BOPTEST scales its flows and loads by the floor multiplier, so floor-2 flows and powers read about ten times the floor-1/3 values. The BACnet side reports the numbers as BOPTEST gives them, no rescaling.

### 3.2 Object type and unit rules

| BOPTEST item | BACnet object | Notes |
|---|---|---|
| measurement `..._y`, continuous | AI | Present_Value = BOPTEST value, no conversion |
| measurement `floorN_reaAHU_occ_y` | BI | 1 = ACTIVE (occupied), 0 = INACTIVE |
| overwrite input `..._u` | AO (commandable) | Min/Max_Pres_Value from BOPTEST (see CSV); Relinquish_Default mirrors the live baseline value; 16-slot priority array |
| `..._activate` | none | derived from the priority array (section 4) |
| `time` | AI on gateway | seconds since 1 Jan 00:00, as BOPTEST reports it |
| `advance` | AO on gateway | optional on-command stepping, same meaning as the stock proxy |
| host-derived G36 signal | AI (integers and reals), BI (flags), MSI (operation mode, 7 states) | updated locally by the host |

Unit mapping (BOPTEST values are already SI, so there is **no numeric conversion** anywhere). Enum values are from `bacnet-types/src/enums/units.rs`.

| BOPTEST unit | BACnet EngineeringUnits | Enum | Used for |
|---|---|---|---|
| K | DEGREES_KELVIN | 63 | all temperatures and temperature setpoints |
| Pa | PASCALS | 53 | static pressure, differential pressure, atmospheric pressure |
| m3/s | CUBIC_METERS_PER_SECOND | 85 | airflows |
| W | WATTS | 47 | electrical power |
| kg/s | KILOGRAMS_PER_SECOND | 42 | water mass flow |
| ppm | PARTS_PER_MILLION | 96 | CO2 |
| 1 | NO_UNITS | 95 | valve/damper/fan fractions (0..1), relative humidity (0..1), sky cover, derived counts |
| s | SECONDS | 73 | time, solar time, day number in seconds |
| rad | RADIANS | 103 | latitude, longitude, solar angles, wind direction |
| W/m2 | WATTS_PER_SQUARE_METER | 35 | solar and IR radiation |
| m/s | METERS_PER_SECOND | 74 | wind speed |
| m | METERS | 31 | cloud ceiling height |

Design choice: valve, damper and fan fractions stay 0..1 with NO_UNITS so that the BACnet value is identical to the BOPTEST value. Showing them as PERCENT (enum 98) would need a x100 conversion in both directions and a second clamp; it is a presentation option for the UI, not for the protocol layer.

## 4. Write semantics: BACnet to BOPTEST `_u` / `_activate`

Per control tick the host does the following for each of the 91 AOs (same behavior as the stock proxy, made explicit):

| Priority array state | Sent to `/advance` | Effect in BOPTEST |
|---|---|---|
| all 16 slots NULL | `<name>_activate = 0` (and no `_u`) | baseline controller in charge |
| at least one slot set | `<name>_u = value of the lowest-numbered (highest-priority) slot`, `<name>_activate = 1` | overwritten for this step |

Rules and recommendations:

1. **Priority convention (proposal).** The G36 supervisory sequences write their 10 outputs at priority **10**. An operator override uses priority **8** (manual operator) and wins over the sequences. A plain WriteProperty from a third-party client with no priority lands in slot 16 and loses to the sequences. This keeps the demo story simple: the sequence is in charge, a human can take over, and releasing the human slot hands control back to the sequence rather than to the BOPTEST baseline.
2. **Sticky activation.** Because `_activate` is sticky in BOPTEST, always send the explicit 0 on release, as the stock proxy does for all 91 inputs on every refresh. Sending only the changes is an optimization that needs per-point state in the host; the full set is about 91 to 182 small keys per step.
3. **Clamp in the host.** BOPTEST truncates out-of-range `_u` with a warning, but that warning is easy to miss. The host should clamp to the min/max in `bacnet-objects.csv` before sending and, ideally, refuse the BACnet write with a value-out-of-range error. Rusty BACnet's AO takes no range from `add_analog_output` in the signatures checked, so the check belongs in the host (open item).
4. **Show the baseline.** When no slot is set, set the AO Relinquish_Default to the `_u` value in the latest `/advance` payload so a BACnet client reading Present_Value sees the live baseline setpoint (zone setpoints, fan speeds, damper positions). This replaces the stock proxy's direct assignment of Present_Value. Use `write_property_local(RELINQUISH_DEFAULT)`; this has not been run against a commandable AO yet, so verify it in the first smoke test.
5. **Order inside one tick.** (a) apply the previous `/advance` (or `/initialize`) response to the BACnet objects and to the engine inputs; (b) run one engine frame; (c) write the 10 supervisory outputs to the AOs at priority 10; (d) read the 91 priority arrays, build the `_u`/`_activate` payload; (e) POST `/advance`. With this order the BACnet view is never staler than the controller's view.
6. **Which `_u` the loop writes.** Exactly these 10: `floor1_TSupAirSet_u`, `floor2_TSupAirSet_u`, `floor3_TSupAirSet_u`, `floor1_dpSet_u`, `floor2_dpSet_u`, `floor3_dpSet_u`, `oveChiWatSys_TWSet_u`, `oveChiWatSys_dpSet_u`, `oveHotWatSys_TWSet_u`, `oveHotWatSys_dpSet_u`. Ranges: AHU SAT 285.15..313.15 K, AHU duct static 50..410 Pa, CHW supply 278.15..288.15 K, HW supply 291.15..353.15 K, plant dp 0..19,130,000 Pa (the upper bound is a placeholder, 40 x the pump head of 478,250 Pa; see the point map for the tighter clamp).
7. **`advance` object.** Optional. If kept, the Rust host can ignore it when it drives stepping itself; if an external BACnet client should be able to step the simulation, honor the stock semantics (step once per increment of the value).

## 5. Why only input-type objects for derived points

Rusty BACnet's local update call for host-computed values, `set_present_value_local`, accepts Analog, Binary and Multi-state Inputs, and also non-commandable Analog, Binary and Multi-state Values built with `PresentValueAccess::ReadOnly` or `Writable` (checked in the source at the pinned rev `1caf4db7`); it rejects Analog Outputs and commandable Values, and `AnalogValueObject::new` is commandable by default. Input-type objects for the 117 derived signals therefore need only that one call per point per tick and are read-only to peers by construction, so a BACnet operator cannot overwrite an intermediate signal. Read-only Analog Values work equally well; the mirror that was built (`docs/bacnet-mirror.md`) uses them for sequence outputs.

## 6. Object tables for each device class

The full listing is `bacnet-objects.csv` (414 rows). Below, one device of each class; the other devices of the class have the same shape.
Column "Legacy proxy inst." is the object instance in the stock proxy's single-device layout (blank for derived points).

### 6.1 AHU class, shown for AHU-1 (device 2001; AHU-2 = 2002, AHU-3 = 2003 differ only by the `floorN_` prefix)

| Object | Object_Name | Units (enum) | BOPTEST point mirrored | Direction / range | Legacy proxy inst. |
|---|---|---|---|---|---|
| AI,1 | `floor1_reaAHU_TSup_y` | DEGREES_KELVIN (63) | `floor1_reaAHU_TSup_y` | read | 3 |
| AI,2 | `floor1_reaAHU_TMix_y` | DEGREES_KELVIN (63) | `floor1_reaAHU_TMix_y` | read | 4 |
| AI,3 | `floor1_reaAHU_TRet_y` | DEGREES_KELVIN (63) | `floor1_reaAHU_TRet_y` | read | 5 |
| AI,4 | `floor1_reaAHU_V_flowSup_y` | CUBIC_METERS_PER_SECOND (85) | `floor1_reaAHU_V_flowSup_y` | read | 6 |
| AI,5 | `floor1_reaAHU_V_flowRet_y` | CUBIC_METERS_PER_SECOND (85) | `floor1_reaAHU_V_flowRet_y` | read | 7 |
| AI,6 | `floor1_reaAHU_dp_y` | PASCALS (53) | `floor1_reaAHU_dp_y` | read | 8 |
| AI,7 | `floor1_reaAHU_PFanRet_y` | WATTS (47) | `floor1_reaAHU_PFanRet_y` | read | 9 |
| AI,8 | `floor1_reaAHU_TCooCoiSup_y` | DEGREES_KELVIN (63) | `floor1_reaAHU_TCooCoiSup_y` | read | 10 |
| AI,9 | `floor1_reaAHU_TCooCoiRet_y` | DEGREES_KELVIN (63) | `floor1_reaAHU_TCooCoiRet_y` | read | 11 |
| AI,10 | `floor1_reaAHU_yCooVal_y` | NO_UNITS (95) | `floor1_reaAHU_yCooVal_y` | read | 12 |
| AI,11 | `floor1_reaAHU_yOA_y` | NO_UNITS (95) | `floor1_reaAHU_yOA_y` | read | 14 |
| AI,12 | `floor1_reaAHU_V_flowOA_y` | CUBIC_METERS_PER_SECOND (85) | `floor1_reaAHU_V_flowOA_y` | read | 15 |
| AI,13 | `floor1_reaAHU_CO2_AHUSup_y` | PARTS_PER_MILLION (96) | `floor1_reaAHU_CO2_AHUSup_y` | read | 16 |
| AI,14 | `floor1_reaAHU_CO2_AHUFre_y` | PARTS_PER_MILLION (96) | `floor1_reaAHU_CO2_AHUFre_y` | read | 17 |
| AI,15 | `floor1_reaAHU_CO2_AHURet_y` | PARTS_PER_MILLION (96) | `floor1_reaAHU_CO2_AHURet_y` | read | 18 |
| AI,16 | `floor1_reaAHU_phiAHUSup_y` | NO_UNITS (95) | `floor1_reaAHU_phiAHUSup_y` | read | 19 |
| AI,17 | `floor1_reaAHU_phiAHURet_y` | NO_UNITS (95) | `floor1_reaAHU_phiAHURet_y` | read | 20 |
| AI,18 | `floor1_reaAHU_PFanSup_y` | WATTS (47) | `floor1_reaAHU_PFanSup_y` | read | 21 |
| AI,19 | `floor1_reaAHU_PFreCoi_y` | WATTS (47) | `floor1_reaAHU_PFreCoi_y` | read | 22 |
| AI,20 | `floor1_reaAHU_TFreCoiLea_y` | DEGREES_KELVIN (63) | `floor1_reaAHU_TFreCoiLea_y` | read | 23 |
| AI,21 | `floor1_g36_ZonTemResReq` | NO_UNITS (95) | (host-derived) | derived |  |
| AI,22 | `floor1_g36_ZonPreResReq` | NO_UNITS (95) | (host-derived) | derived |  |
| AI,23 | `floor1_g36_ChiWatResReq` | NO_UNITS (95) | (host-derived) | derived |  |
| AI,24 | `floor1_g36_ChiPlaReq` | NO_UNITS (95) | (host-derived) | derived |  |
| AI,25 | `floor1_g36_TSupSetMax` | DEGREES_KELVIN (63) | (host-derived) | derived |  |
| BI,1 | `floor1_reaAHU_occ_y` | NO_UNITS (95) | `floor1_reaAHU_occ_y` | read | 13 |
| BI,2 | `floor1_g36_SupFanSta` | NO_UNITS (95) | (host-derived) | derived |  |
| MSI,1 | `floor1_g36_OpeMod` | NO_UNITS (95) | (host-derived) | derived |  |
| AO,1 | `floor1_duaFanAirHanUni_supFan_oveSpeSupFan_u` | NO_UNITS (95) | `floor1_duaFanAirHanUni_supFan_oveSpeSupFan_u` | writes `_u` + `_activate`; range 0..1 | 207 |
| AO,2 | `floor1_duaFanAirHanUni_cooCoi_yCoo_u` | NO_UNITS (95) | `floor1_duaFanAirHanUni_cooCoi_yCoo_u` | writes `_u` + `_activate`; range 0..1 | 208 |
| AO,3 | `floor1_duaFanAirHanUni_mixingBox_mixBox_yOA_u` | NO_UNITS (95) | `floor1_duaFanAirHanUni_mixingBox_mixBox_yOA_u` | writes `_u` + `_activate`; range 0..1 | 209 |
| AO,4 | `floor1_duaFanAirHanUni_mixingBox_mixBox_yRet_u` | NO_UNITS (95) | `floor1_duaFanAirHanUni_mixingBox_mixBox_yRet_u` | writes `_u` + `_activate`; range 0..1 | 210 |
| AO,5 | `floor1_duaFanAirHanUni_mixingBox_mixBox_yEA_u` | NO_UNITS (95) | `floor1_duaFanAirHanUni_mixingBox_mixBox_yEA_u` | writes `_u` + `_activate`; range 0..1 | 211 |
| AO,6 | `floor1_duaFanAirHanUni_oveSpeRetFan_u` | NO_UNITS (95) | `floor1_duaFanAirHanUni_oveSpeRetFan_u` | writes `_u` + `_activate`; range 0..1 | 212 |
| AO,7 | `floor1_duaFanAirHanUni_frePro_oveFrePro_u` | NO_UNITS (95) | `floor1_duaFanAirHanUni_frePro_oveFrePro_u` | writes `_u` + `_activate`; range 0..1 | 213 |
| AO,8 | `floor1_TSupAirSet_u` | DEGREES_KELVIN (63) | `floor1_TSupAirSet_u` | writes `_u` + `_activate`; range 285.15..313.15 | 224 |
| AO,9 | `floor1_dpSet_u` | PASCALS (53) | `floor1_dpSet_u` | writes `_u` + `_activate`; range 50..410 | 225 |

### 6.2 VAV box class, shown for VAV-1-COR (device 3011; the other 14 differ by floor and zone tokens)

| Object | Object_Name | Units (enum) | BOPTEST point mirrored | Direction / range | Legacy proxy inst. |
|---|---|---|---|---|---|
| AI,1 | `floor1_reaZonCor_TZon_y` | DEGREES_KELVIN (63) | `floor1_reaZonCor_TZon_y` | read | 24 |
| AI,2 | `floor1_reaZonCor_TSup_y` | DEGREES_KELVIN (63) | `floor1_reaZonCor_TSup_y` | read | 25 |
| AI,3 | `floor1_reaZonCor_V_flow_y` | CUBIC_METERS_PER_SECOND (85) | `floor1_reaZonCor_V_flow_y` | read | 26 |
| AI,4 | `floor1_reaZonCor_yReheaVal_y` | NO_UNITS (95) | `floor1_reaZonCor_yReheaVal_y` | read | 27 |
| AI,5 | `floor1_reaZonCor_yDam_y` | NO_UNITS (95) | `floor1_reaZonCor_yDam_y` | read | 28 |
| AI,6 | `floor1_reaZonCor_V_flowSet_y` | CUBIC_METERS_PER_SECOND (85) | `floor1_reaZonCor_V_flowSet_y` | read | 29 |
| AI,7 | `floor1_reaZonCor_CO2Zon_y` | PARTS_PER_MILLION (96) | `floor1_reaZonCor_CO2Zon_y` | read | 30 |
| AI,8 | `floor1_g36_Cor_CooLoo` | NO_UNITS (95) | (host-derived) | derived |  |
| AI,9 | `floor1_g36_Cor_ZonTemResReq` | NO_UNITS (95) | (host-derived) | derived |  |
| AI,10 | `floor1_g36_Cor_ZonPreResReq` | NO_UNITS (95) | (host-derived) | derived |  |
| AI,11 | `floor1_g36_Cor_HotWatResReq` | NO_UNITS (95) | (host-derived) | derived |  |
| AI,12 | `floor1_g36_Cor_HotWatPlaReq` | NO_UNITS (95) | (host-derived) | derived |  |
| BI,1 | `floor1_g36_Cor_AftSup` | NO_UNITS (95) | (host-derived) | derived |  |
| AO,1 | `floor1_fivZonVAV_vAV1_oveZonLoc_yDam_u` | NO_UNITS (95) | `floor1_fivZonVAV_vAV1_oveZonLoc_yDam_u` | writes `_u` + `_activate`; range 0..1 | 214 |
| AO,2 | `floor1_fivZonVAV_vAV1_oveZonLoc_yReaHea_u` | NO_UNITS (95) | `floor1_fivZonVAV_vAV1_oveZonLoc_yReaHea_u` | writes `_u` + `_activate`; range 0..1 | 215 |
| AO,3 | `floor1_oveZonCor_TZonHeaSet_u` | DEGREES_KELVIN (63) | `floor1_oveZonCor_TZonHeaSet_u` | writes `_u` + `_activate`; range 285.15..313.15 | 226 |
| AO,4 | `floor1_oveZonCor_TZonCooSet_u` | DEGREES_KELVIN (63) | `floor1_oveZonCor_TZonCooSet_u` | writes `_u` + `_activate`; range 285.15..313.15 | 227 |

### 6.3 Chilled-water plant (device 1001)

| Object | Object_Name | Units (enum) | BOPTEST point mirrored | Direction / range | Legacy proxy inst. |
|---|---|---|---|---|---|
| AI,1 | `reaChiWatSys_dp_y` | PASCALS (53) | `reaChiWatSys_dp_y` | read | 194 |
| AI,2 | `reaChiWatSys_TCHWSup_y` | DEGREES_KELVIN (63) | `reaChiWatSys_TCHWSup_y` | read | 195 |
| AI,3 | `reaChiWatSys_reaPPum_y` | WATTS (47) | `reaChiWatSys_reaPPum_y` | read | 196 |
| AI,4 | `reaChiWatSys_reaPChi_y` | WATTS (47) | `reaChiWatSys_reaPChi_y` | read | 197 |
| AI,5 | `reaChiWatSys_reaPCooTow_y` | WATTS (47) | `reaChiWatSys_reaPCooTow_y` | read | 198 |
| AI,6 | `reaChiWatSys_TCHWRet_y` | DEGREES_KELVIN (63) | `reaChiWatSys_TCHWRet_y` | read | 199 |
| AI,7 | `reaChiWatSys_mCHWTot_y` | KILOGRAMS_PER_SECOND (42) | `reaChiWatSys_mCHWTot_y` | read | 200 |
| AI,8 | `g36_ChiWatResReqSum` | NO_UNITS (95) | (host-derived) | derived |  |
| AI,9 | `g36_ChiPlaReqSum` | NO_UNITS (95) | (host-derived) | derived |  |
| BI,1 | `g36_ChiWatPlaSta` | NO_UNITS (95) | (host-derived) | derived |  |
| AO,1 | `oveChiWatSys_TWSet_u` | DEGREES_KELVIN (63) | `oveChiWatSys_TWSet_u` | writes `_u` + `_activate`; range 278.15..288.15 | 294 |
| AO,2 | `oveChiWatSys_dpSet_u` | PASCALS (53) | `oveChiWatSys_dpSet_u` | writes `_u` + `_activate`; range 0..19130000 | 295 |

### 6.4 Hot-water plant (device 1002)

| Object | Object_Name | Units (enum) | BOPTEST point mirrored | Direction / range | Legacy proxy inst. |
|---|---|---|---|---|---|
| AI,1 | `reaHotWatSys_dp_y` | PASCALS (53) | `reaHotWatSys_dp_y` | read | 201 |
| AI,2 | `reaHotWatSys_THWSup_y` | DEGREES_KELVIN (63) | `reaHotWatSys_THWSup_y` | read | 202 |
| AI,3 | `reaHotWatSys_reaPPum_y` | WATTS (47) | `reaHotWatSys_reaPPum_y` | read | 203 |
| AI,4 | `reaHotWatSys_reaPBoi_y` | WATTS (47) | `reaHotWatSys_reaPBoi_y` | read | 204 |
| AI,5 | `reaHotWatSys_THWRet_y` | DEGREES_KELVIN (63) | `reaHotWatSys_THWRet_y` | read | 205 |
| AI,6 | `reaHotWatSys_mHWTot_y` | KILOGRAMS_PER_SECOND (42) | `reaHotWatSys_mHWTot_y` | read | 206 |
| AI,7 | `g36_HotWatResReqSum` | NO_UNITS (95) | (host-derived) | derived |  |
| AI,8 | `g36_HotWatPlaReqSum` | NO_UNITS (95) | (host-derived) | derived |  |
| BI,1 | `g36_HotWatPlaSta` | NO_UNITS (95) | (host-derived) | derived |  |
| AO,1 | `oveHotWatSys_TWSet_u` | DEGREES_KELVIN (63) | `oveHotWatSys_TWSet_u` | writes `_u` + `_activate`; range 291.15..353.15 | 296 |
| AO,2 | `oveHotWatSys_dpSet_u` | PASCALS (53) | `oveHotWatSys_dpSet_u` | writes `_u` + `_activate`; range 0..19130000 | 297 |

### 6.5 Weather station (device 1000)

| Object | Object_Name | Units (enum) | BOPTEST point mirrored | Direction / range | Legacy proxy inst. |
|---|---|---|---|---|---|
| AI,1 | `weaSta_reaWeaTDryBul_y` | DEGREES_KELVIN (63) | `weaSta_reaWeaTDryBul_y` | read | 171 |
| AI,2 | `weaSta_reaWeaRelHum_y` | NO_UNITS (95) | `weaSta_reaWeaRelHum_y` | read | 172 |
| AI,3 | `weaSta_reaWeaWinSpe_y` | METERS_PER_SECOND (74) | `weaSta_reaWeaWinSpe_y` | read | 173 |
| AI,4 | `weaSta_reaWeaWinDir_y` | RADIANS (103) | `weaSta_reaWeaWinDir_y` | read | 174 |
| AI,5 | `weaSta_reaWeaHGloHor_y` | WATTS_PER_SQUARE_METER (35) | `weaSta_reaWeaHGloHor_y` | read | 175 |
| AI,6 | `weaSta_reaWeaNTot_y` | NO_UNITS (95) | `weaSta_reaWeaNTot_y` | read | 176 |
| AI,7 | `weaSta_reaWeaPAtm_y` | PASCALS (53) | `weaSta_reaWeaPAtm_y` | read | 177 |
| AI,8 | `weaSta_reaWeaHDifHor_y` | WATTS_PER_SQUARE_METER (35) | `weaSta_reaWeaHDifHor_y` | read | 178 |
| AI,9 | `weaSta_reaWeaCeiHei_y` | METERS (31) | `weaSta_reaWeaCeiHei_y` | read | 179 |
| AI,10 | `weaSta_reaWeaTWetBul_y` | DEGREES_KELVIN (63) | `weaSta_reaWeaTWetBul_y` | read | 180 |
| AI,11 | `weaSta_reaWeaTDewPoi_y` | DEGREES_KELVIN (63) | `weaSta_reaWeaTDewPoi_y` | read | 181 |
| AI,12 | `weaSta_reaWeaTBlaSky_y` | DEGREES_KELVIN (63) | `weaSta_reaWeaTBlaSky_y` | read | 182 |
| AI,13 | `weaSta_reaWeaHHorIR_y` | WATTS_PER_SQUARE_METER (35) | `weaSta_reaWeaHHorIR_y` | read | 183 |
| AI,14 | `weaSta_reaWeaHDirNor_y` | WATTS_PER_SQUARE_METER (35) | `weaSta_reaWeaHDirNor_y` | read | 184 |
| AI,15 | `weaSta_reaWeaCloTim_y` | SECONDS (73) | `weaSta_reaWeaCloTim_y` | read | 185 |
| AI,16 | `weaSta_reaWeaSolAlt_y` | RADIANS (103) | `weaSta_reaWeaSolAlt_y` | read | 186 |
| AI,17 | `weaSta_reaWeaNOpa_y` | NO_UNITS (95) | `weaSta_reaWeaNOpa_y` | read | 187 |
| AI,18 | `weaSta_reaWeaLat_y` | RADIANS (103) | `weaSta_reaWeaLat_y` | read | 188 |
| AI,19 | `weaSta_reaWeaLon_y` | RADIANS (103) | `weaSta_reaWeaLon_y` | read | 189 |
| AI,20 | `weaSta_reaWeaSolDec_y` | RADIANS (103) | `weaSta_reaWeaSolDec_y` | read | 190 |
| AI,21 | `weaSta_reaWeaSolHouAng_y` | RADIANS (103) | `weaSta_reaWeaSolHouAng_y` | read | 191 |
| AI,22 | `weaSta_reaWeaSolTim_y` | SECONDS (73) | `weaSta_reaWeaSolTim_y` | read | 192 |
| AI,23 | `weaSta_reaWeaSolZen_y` | RADIANS (103) | `weaSta_reaWeaSolZen_y` | read | 193 |

### 6.6 Gateway (device 599)

| Object | Object_Name | Units (enum) | BOPTEST point mirrored | Direction / range | Legacy proxy inst. |
|---|---|---|---|---|---|
| AI,1 | `time` | SECONDS (73) | `time` | read | 2 |
| AO,1 | `advance` | NO_UNITS (95) | `advance` | writes `_u` + `_activate`; range 0.. | 1 |

## 7. Rusty BACnet creation recipe

One `BACnetServer` per device (the docs show one server per device identity; a router transport lets many servers share a host). Calls from the Python binding; the Rust crates expose the same operations.

| Object | Call | Notes |
|---|---|---|
| AI | `server.add_analog_input(instance, name, units, present_value)` | updated with `set_present_value_local` |
| BI | `server.add_binary_input(instance, name)` | updated with `set_present_value_local` (0/1) |
| MSI | `server.add_multistate_input(instance, name, number_of_states)` | operation mode uses 7 states |
| AO | `server.add_analog_output(instance, name, units)` | commandable; reads of the priority array through `server.read_property(object_id, PRIORITY_ARRAY)` |

Open decisions for the transport: 22 servers need either 22 UDP ports (for example 47808 + n) on one machine, or one virtual router with a network number per device group. Discovery behavior depends on the transport chosen and was not tested here. This is a transport detail and does not change object identifiers.

## 8. Risks and open items

1. **Priority-array read cost and API.** The host reads 91 priority arrays per tick. At a 120 s control step that is trivial. The server API was not searched exhaustively for a write callback that would avoid polling; polling the local server is fine at this rate.
2. **AO range enforcement** is not provided by the creation call; the host must clamp and reject (section 4, item 3).
3. **Relinquish_Default mirroring** for live baseline values is untested (section 4, item 4). Fallback: expose the baseline in a separate read-only AI per input, which doubles the objects for the 10 supervisory points only.
4. **One shared HW plant dp upper bound** (19.13 MPa) and the CHW one are placeholders; a BACnet client can write absurd values that BOPTEST will accept. Clamp tighter in the host (see the point map).
5. **Derived signals depend on assumptions** (fan status from airflow, plant status from power or flow). They are labeled in the CSV description column as derived; do not present them as BOPTEST measurements in the demo.
6. **Floor-2 multiplier.** Mixing floor-2 and floor-1/3 numbers in one plant or sum-of-requests view is misleading; equal weighting per AHU is the G36 convention but floor 2 carries about ten times the load.
7. **Demo scope.** The stock proxy and the BOPTEST point list are existing work; the new work to claim is the device layout, the G36 sequence wiring, the host and the derived objects. Keep the 91 pass-through AOs visible as "parity", not as a feature.
