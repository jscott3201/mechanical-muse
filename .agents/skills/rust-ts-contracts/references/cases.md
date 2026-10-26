# Contract fixture cases

Use the project's fixture directory and one JSON file per case family. Each case records the input, accept or reject, and the semantic value or error kind. Expected values are written by hand from the contract, not produced by the code under test.

| Input shape | Decision to make explicit |
|---|---|
| Real `NaN`, `Infinity`, `-Infinity` | Serialized as strings; the TS decoder returns the same non-finite number; plain JSON `NaN` is rejected. |
| Integer `9007199254740993` | Reject it as a JS number, or carry it as a string by the per-field decision. Never round it silently. |
| Enum ordinal `0` or past the class size | Ordinals are 1-based; reject out-of-range values with a typed error. |
| Absent field, `null`, `0`, `false` | Four distinct meanings for telemetry; assert the serialized JSON, not only the Rust or TS object. |
| Unknown SSE `kind` or new optional field | TS ignores it and keeps the stream alive; the Rust side keeps a catch-all arm where it parses. |
| Duplicate point id or an id that is not a CXF `@id` | Reject at the DTO boundary; never key the UI on a derived display name. |
| Unit and time meaning | BOPTEST values are SI (K, Pa, m3/s); `sim_time_s` is simulation seconds, never wall clock. Derived signals are labelled as derived. |
| Oversized or deeply nested payload | Rejected with a typed error at the real public boundary (CXF limits: 8 MiB, 64 levels). |
| Error body | Carries `kind`, `path` and a human message; never the request body, prompt or key. |

Drift check: after any DTO change, regenerate the TS types, review the diff, and confirm the web typecheck still passes. A compiling consumer is not evidence that the runtime payload matches.

<!-- Adapted from yet-more-skills/cross-language-contracts and yet-more-skills/typescript-contracts (MIT OR Apache-2.0) -->
