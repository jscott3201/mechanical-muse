---
name: diff-review
description: "Guides a read-only review of the working-tree diff before the human commits. Use when asked to review changes; checks hard rules, determinism, tests and claims."
license: MIT OR Apache-2.0
metadata:
  short-description: "Review the working-tree diff without editing"
---
# Working-Tree Diff Review

Review the uncommitted changes (and untracked files) in context: open callers, tests and `AGENTS.md`, not just changed lines. Stay read-only: no edits, staging or commits. If unrelated edits blur attribution, say so.

## Severity
- **blocker**: hard-rule break, secret leak, data loss, wrong result, failing fmt, clippy or tests.
- **major**: likely bug, unbounded queue or allocation, nondeterminism, behavior change without a test.
- **nit**: style or preference; last.

## Hard-rule checklist
- Cargo: changed git `rev`, `[patch]`, path override, vendored upstream source, `cargo update` churn.
- Determinism: `HashMap` order, wall-clock time or unseeded randomness in emitters, validators, verification or runtime.
- Panics and logs: `unwrap`, `expect` or indexing outside tests; an `OcError` match with no wildcard arm; `println!` outside binaries; secrets in `tracing`.
- Hygiene: keys, `.env`, absolute paths, personal emails, session ids, raw transcripts; links from committed files to gitignored places (`scratch/`, local research notes).
- Provenance: a new dependency, dataset, vendored UI component or forked skill with no README "Pre-existing dependencies" row in the same change; old behavior presented as new.
- Standards and claims: ASHRAE G36, 231P or 135 text, tables or figures; "verified", "compliant" or endorsement wording in README, UI copy or demo script.
- Trust: model output reaching BOPTEST or BACnet without a green receipt; one `Engine` shared by authoring and runtime; switching on diagnostic message text.
- Engine contract: partial or retried frames, `halt`/`set_param`/`resume` for live tuning, f32 quantization outside `mm-bacnet`, BACnet writes not deny-all.
- Web: `any`, hand-written TS mirrors of Rust types, restyled shadcn components, an API key in the bundle.

## Evidence
- Check that tests cover the affected crate and read their assertions. A golden regenerated without a reviewed diff, or a test a wrong implementation would pass, counts as missing.
- Try to disprove each suspect with existing guards first. A finding gives file and symbol, trigger, consequence and the smallest fix or test; unverified risks are questions; report each root cause once.
- Lead with findings by severity, then confidence, unrun checks and a changed-files list grouped by logical commit with scope-prefixed subjects. Clean means no supported issue found in scope, not certification.
- Prompts: [review prompts](references/prompts.md)

<!-- Adapted from yet-more-skills/rust-review and yet-more-skills/immutable-pr-review (MIT OR Apache-2.0) -->
