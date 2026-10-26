---
name: mm-bacnet-mirror
description: "Guides mm-bacnet: mapping engine points to rusty-bacnet objects, f32 quantization, local writes, write policy, device numbering. Use when changing the point map, device layout or BACnet tests."
license: MIT OR Apache-2.0
metadata:
  short-description: "rusty-bacnet mirror: point map, local writes, policy"
---
`bacnet-objects`, `bacnet-server`, `bacnet-types` at rusty-bacnet v0.14.0 (`1caf4db7`, read-only); needs a Tokio runtime. Depth: `docs/engine-api.md` section 14, `docs/bacnet-device-table.md`.

**Local update calls** (checked in source at the pinned rev; this settles the conflict between `docs/engine-api.md` section 14 and `docs/bacnet-device-table.md` section 5, which wrongly rules out AV)
- `set_present_value_local(&oid, value)` works for AI, BI, MSI and non-commandable Values: AV, BV, MSV built with `PresentValueAccess::ReadOnly` or `Writable`. It fails with OBJECT / OPTIONAL_FUNCTIONALITY_NOT_SUPPORTED on AO and on commandable Values (`AnalogValueObject::new` is commandable by default). NaN or infinity gives PROPERTY / VALUE_OUT_OF_RANGE; Out_Of_Service TRUE denies it.
- Value types (read from source, binary and multistate not run): analog `Real(f32)`, binary `Enumerated(0|1)`, multistate `Unsigned(1..=states)`.
- `write_local(&oid, PRESENT_VALUE, None, value, Some(priority), LocalCommandSource::ServerDevice)` is for AO. Sequence outputs go at priority 10, operator overrides at 8 (device-table convention).
- Computed or derived points: AI/BI/MSI, or a ReadOnly AV; commanded outputs: AO.

**Rules**
- Quantize f64 to f32 once, in `mm-bacnet`; tests compare against the engine f64 with an f32 tolerance.
- Pass `.mutation_policy(MutationPolicy::DenyAll)` to the builder: the library default is `Permissive`. Local calls still work under DenyAll. For an operator-override demo use `Permissive` plus a `FnPolicy` allowing writes only to the override objects, read them back with `read_local`, and never open network writes to AOs the sequence owns.
- Generate the map from `engine.point_list(None)` plus the BOPTEST points, keyed by CXF `@id`. Instance numbers are per device and object type: the 1-based rank in sorted `@id` order, stable across runs and tested. Object names are unique per device; mirrored points keep their BOPTEST names; derived points carry a `g36_` name part and are labelled derived.
- Device ids come from the device table (proposal, final ids pending in `docs/DECISIONS.md`): 599 gateway, 1000 weather, 1001 CHW plant, 1002 HW plant, 2000+floor AHU, 3000+10*floor+zone VAV with zones 1..5 = Cor, Sou, Eas, Nor, Wes. Vendor id is a labelled placeholder constant.
- Share the server in an `Arc` (local calls take `&self`); `stop().await` needs `&mut`, call it before drop. UDP 47808 for the demo, port 0 in tests; broadcast on a Docker bridge network does not reach the LAN, so use host networking or map `47808/udp`.
- No conformance or certification claims (`mm-hackathon-boundary`).
