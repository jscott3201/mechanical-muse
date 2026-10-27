# BACnet mirror (read-only)

`crates/mm-bacnet` serves live runs as one BACnet/IP device, so a building operator's tools can
see what the verified sequences are doing. It uses rusty-bacnet (pre-existing, pinned; see the
README's dependency table) for the protocol; the point map, the updates and the wiring are ours.

## What it serves

Device `599`, name `MECHANICAL-MUSE`, a placeholder vendor id (0, not one assigned to us).

| Points | Object type | Names | Updated from |
| --- | --- | --- | --- |
| 18 BOPTEST readings shown beside the setpoints (plant, three air handlers) | Analog Input | BOPTEST names, e.g. `reaChiWatSys_TCHWSup_y` | each tick's readings |
| 8 sequence outputs (three AHU request instances, the plant reset) | Analog Value, read-only | `<instance>.<output>`, e.g. `chw.TSupSet`, `ahu2.yChiWatResReq` | each tick's engine outputs (booleans as 0 or 1) |
| 2 plant setpoints the runtime sent | Analog Value, read-only | `oveChiWatSys_TWSet_u`, `oveChiWatSys_dpSet_u` | each tick's writes |

- Instance numbers are the 1-based rank of the name among objects of the same type, so the same
  specs always give the same objects (`crates/mm-bacnet/src/map.rs`, tested).
- Units come from BOPTEST's point list (`docs/boptest-points.csv`): K, Pa, W, kg/s; others are
  unitless.
- Values are `f64` in the engine and BOPTEST and `f32` in BACnet; `mm_bacnet::quantize` is the
  only conversion.
- A point with no finite value on a tick (a NaN reading, a setpoint not sent because the
  sequence is degraded) keeps its last value and reads Reliability `NO_SENSOR` until a value
  arrives; it never shows NaN or a stale value as good.
- Only live runs update it. Replays of recorded runs do not (they are not live data).

## What it refuses

Every remote write is refused (`MutationPolicy::DenyAll`; the library default is permissive).
A peer reading the device sees the sequences' decisions; it cannot change them. The tests write
each object type over UDP with rusty-bacnet's own client and check the refusal and that the
value is unchanged (`crates/mm-bacnet/tests/mirror.rs`).

Not done: the 22-device layout and operator overrides proposed in `docs/bacnet-device-table.md`,
COV subscriptions beyond what the library serves by default, BBMD or foreign-device
registration. No conformance or certification is claimed.

## Run it

```sh
MM_BACNET_ADDR=0.0.0.0:47808 make server      # or make server-live
```

`GET /api/bacnet` lists the device and its objects (404 when the mirror is off); the web app's
run form says when it is on. Any BACnet/IP client on the same network can discover device 599
(Who-Is) and read it. Broadcasts from inside a Docker bridge network do not reach the LAN: run
the server on the host.
