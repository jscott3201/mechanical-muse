# BACnet mirror (read-only)

`crates/mm-bacnet` serves live runs as one BACnet/IP device, so a building operator's tools can
see what the verified sequences are doing. It uses rusty-bacnet (pre-existing, pinned; see the
README's dependency table) for the protocol; the point map, the updates and the wiring are ours.

## What it serves

Device `599`, name `MECHANICAL-MUSE`. The project has no BACnet vendor id; the device reports
65535 as a placeholder (not 0, which belongs to the standard body).

| Points | Object type | Names | Updated from |
| --- | --- | --- | --- |
| 18 BOPTEST values shown beside the setpoints (plant and air handler readings, and the AHU supply air setpoints BOPTEST reports in effect) | Analog Input | BOPTEST names, e.g. `reaChiWatSys_TCHWSup_y` | each tick's readings |
| 8 sequence outputs (three AHU request instances, the plant reset) | Analog Value, read-only | `<instance>.<output>`, e.g. `chw.TSupSet`, `ahu2.yChiWatResReq` | each tick's engine outputs (booleans as 0 or 1) |
| 2 plant setpoints the runtime sent | Analog Value, read-only | `oveChiWatSys_TWSet_u`, `oveChiWatSys_dpSet_u` | each tick's writes |

- Instance numbers are the 1-based rank of the name among objects of the same type, so the same
  specs always give the same objects (`crates/mm-bacnet/src/map.rs`, tested). Adding a spec
  renumbers the Analog Values; a peer that configured points by instance must re-read them.
- Units come from BOPTEST's point list (`docs/boptest-points.csv`): K, Pa, W, kg/s; others are
  unitless.
- Values are `f64` in the engine and BOPTEST and `f32` in BACnet; `mm_bacnet::quantize` is the
  only conversion.
- Reliability `NO_SENSOR` means "nothing produced this value now", not a broken sensor. A
  point reads it, keeping its last value, when its tick carries no finite value, and every point
  reads it before the first run and after each run ends. So a setpoint the runtime did not send
  (a degraded sequence handed back to BOPTEST, a baseline run, a released point) reads
  `NO_SENSOR` during normal operation, and an injected test fault shows only as `NO_SENSOR`; the
  web app's event log says which. A setpoint held by the fail-safe is sent, so it reads reliable.
- One point the device refuses never leaves the others stale: it is flagged and the rest update.
  An update is given 2 s; past that the tick goes on without it (BACnet is a view, not the
  control path).
- Only live runs update it. Replays of recorded runs do not (they are not live data); a test
  checks this.

## What it refuses

Every request that would change an object is refused (`MutationPolicy::DenyAll`; the library
default is permissive): property writes, object creation and deletion, list changes, and COV
subscriptions, so peers poll. A peer sees the sequences' decisions; it cannot change them. The
tests write Out_Of_Service and Object_Name (which the objects would accept under the default
policy) and Present_Value over UDP with rusty-bacnet's own client, check the deny-all error and
the unchanged values, and fail if the policy is switched back to permissive
(`crates/mm-bacnet/tests/mirror.rs`). Time synchronization is not an object change and is
accepted as the library does by default; it changes no point.

Not done: the 22-device layout and operator overrides proposed in `docs/bacnet-device-table.md`,
COV, BBMD or foreign-device registration. No conformance or certification is claimed.

## Run it

```sh
MM_BACNET_ADDR=0.0.0.0:47808 make server      # or make server-live
```

`GET /api/bacnet` lists the device and its objects (404 when the mirror is off); the web app's
run form says when it is on. Any BACnet/IP client on the same network can discover device 599
(Who-Is) and read it. Broadcasts from inside a Docker bridge network do not reach the LAN: run
the server on the host.
