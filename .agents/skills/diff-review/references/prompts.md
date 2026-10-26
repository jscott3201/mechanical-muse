# Focused review prompts

Apply these to the changed boundary only; they are questions, not a lint list.

**Identity:** Is a point keyed by its CXF `@id` everywhere (frames, trends, BACnet mapping, UI), or did a second name sneak in? Can a stale reply or old frame plan affect the current run?

**Failure atomicity:** After a refused frame, a cancelled await or a failed write, which state is visible? A refusal must leave the engine unchanged and must not write setpoints.

**Ownership:** Does one task own each `Engine`? Does a borrowed or cloned value outlive its owner, and did shared mutation appear only to dodge a borrow error?

**Validation scope:** Did the tests that ran include the changed crate and the web consumer, or only a cheap subset? Were `#[ignore]` tests assumed green rather than run?

**Oracle independence:** Was the expected output derived from the spec, or just emitted by the changed code? A constant-false graph passes 40.9% of library scenarios, so check that a wrong implementation would fail.

**Numeric meaning:** Are comparisons on `Value` done with `bit_eq`? Is NaN handled in the host, f32 quantization done once in `mm-bacnet`, and tolerance applied only to reals?

**Web boundary:** Strict TypeScript with no `any` or non-null assertion hiding uncertainty; no `as` cast standing in for parsing; SSE and fetch logic in hooks with cleanup on unmount (a double mount in development must not open two streams); list keys are point ids, not array indexes.

**Claims:** Does new README, UI or demo text say more than a test or measurement shows? Does it separate what was built in the event from the pre-existing dependencies?

<!-- Adapted from yet-more-skills/rust-review and yet-more-skills/immutable-pr-review (MIT OR Apache-2.0) -->
