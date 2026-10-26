# Vector and benchmark notes

## Scenario shape

A vector file holds a clock (`step_s`, `horizon_s`) and scenarios. A scenario gives each input as a constant or a list of `{t, value}` steps, plus `expect` windows `{output, from_s, to_s, equals}`.

Worked pattern, a two-condition fault with a persistence delay (60 s steps, 900 s delay): inputs steady, the fault condition becomes true at t=0; assert the fault output false for 0 to 840, leave 841 to 959 as don't-care for the delay edge, assert true from 960. Add a scenario where one condition drops before the delay ends and assert the output stays false. Add one where the inputs start already true to expose the `delayOnInit` convention.

## Checks every vector set should survive

- Constant-false and constant-true graphs each fail at least one scenario.
- One mutation each of threshold, connection and delay turns some scenario red.
- Load twice, replay twice: identical outputs under `bit_eq` and an identical export `content_id`.
- Each scenario runs on a fresh engine; the harness holds the last value of an input and submits the full table every frame.

## Benchmark hygiene (library rules as data)

- Use the fixed split in `docs/benchmark-split.md` and `docs/benchmark-split.json`. Headline numbers use `test_clean`; generalization claims use `test_strict`. Decide the tier before reporting.
- Headline metric is rule-level all-scenario pass; show scenario pass rate and the constant-false baseline next to it, and keep load failures separate.
- Scrub prompts: drop `params.*.cxf`, diagrams, and the Deviations and Notes sections, which leak the answer. Exemplars come from train only.
- Keep the vectors hidden at generation time; a second fresh set, or a few newly written rules, guards against contamination.
- Record model id, reasoning effort and seeds. Use the `-contributor` model only on public library data. Judge with the same pinned engine rev as the palette.
- Do not claim more than the vectors show: they are author-written scenarios on one engine revision, not formal equivalence.

<!-- Adapted from yet-more-skills/building-fdd-validation (MIT OR Apache-2.0) -->
