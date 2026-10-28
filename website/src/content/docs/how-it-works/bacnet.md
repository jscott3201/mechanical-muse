---
title: BACnet mirror
description: Live runs served read-only as one BACnet/IP device, so an operator's tools can see what the verified sequences are doing.
---

`mm-bacnet` serves live runs as one BACnet/IP device. It uses rusty-bacnet (pre-existing,
pinned, used unmodified) for the protocol; the point map, the updates and the wiring are ours.

## What it serves

Device `599`, name `MECHANICAL-MUSE`, 55 objects:

| Points | Object type | Example names |
| --- | --- | --- |
| 24 BOPTEST readings (plant and air handler values, duct static pressures, supply fan power) | Analog Input | `reaChiWatSys_TCHWSup_y`, `floor2_reaAHU_PFanSup_y` |
| 26 sequence outputs | Analog Value, read-only | `chw.TSupSet`, `zone2eas.yPreReq`, `sp1.dpSet` |
| 5 setpoints the runtime sent | Analog Value, read-only | `oveChiWatSys_TWSet_u`, `floor1_dpSet_u` |

- Instance numbers derive from the sorted names, so the same specs always give the same objects.
- Values are `f64` in the engine and `f32` in BACnet, converted in exactly one place.
- A point with nothing to report (before a run, after it, a setpoint handed back to BOPTEST, a
  blank test fault) reads reliability `NO_SENSOR`.
- Only live runs update it; replays of recorded runs do not.

## What it refuses

Every request that would change an object is refused: property writes, object creation and
deletion, list changes, and COV subscriptions (so peers poll). A peer sees the sequences'
decisions; it cannot change them. Tests send writes over UDP with rusty-bacnet's own client and
fail if the policy is ever switched back to permissive. No conformance or certification is
claimed.

## Run it

```sh
MM_BOPTEST_URL=http://127.0.0.1:80 MM_BACNET_ADDR=0.0.0.0:47808 make server
```

Any BACnet/IP client on the same network can discover device 599 (Who-Is) and read it.
BACnet/IP relies on UDP broadcast, which Docker's default network does not carry to the LAN, so
run the server natively for the mirror.

## Learn more

- [docs/bacnet-mirror.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/bacnet-mirror.md):
  every object, reliability and status flags, and how to check it with a BACnet client.
