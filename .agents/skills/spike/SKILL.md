---
name: spike
description: "Guides a time-boxed throwaway experiment that answers one open question (BOPTEST speed, Meta API behavior, a probe). Use only when asked to spike; results go to docs/ or are discarded."
license: MIT OR Apache-2.0
metadata:
  short-description: "Time-boxed throwaway experiment"
---
# Spike

Run the smallest experiment that can change a decision. Use it only when asked; a spike is not production delivery.

## Frame it
- Before coding, state the question, the observation that would decide it, and a time box (minutes or a call budget). At the box, stop and report what is known; a clear negative result is a finished spike.
- Typical questions: the BOPTEST speed benchmark (steps per second at the intended step size, local Rosetta versus an x86 VM, which settles compose layout, step size and demo length); items in the uncertainty ledger in `docs/model-api.md` (structured output beside tools, schema keyword support, refusal shapes); a trim-and-respond probe with `have_hol=true` (`docs/plant-reset.md`); BACnet object behavior through `rusty-bacnet` (`docs/engine-api.md`).

## Keep it isolated
- Code lives in gitignored `scratch/` as its own tiny project outside the workspace members. Do not wire it into a crate, add `[patch]`, change a pinned rev or touch `Cargo.lock`.
- Keys come from the environment only, never from files. Use `muse-spark-1.3-contributor` only with public data. Respect the shared rate limits and say which live service the spike calls.
- Prefer fixtures and offline inputs wherever the question allows; label mock data and unsupported paths.

## Measure honestly
- Compare like with like: release builds, warm-up, identical inputs, at least three runs, median and spread. Separate setup from the operation measured. Record the host (arch, Rosetta or native), versions and pinned revs.
- Test the case that would falsify the idea, not only the happy path.

## Close it
- Record the answer in `docs/` in your own words: question, method, observation, conditions and the pending decision in `AGENTS.md` it informs. Scrub keys, absolute paths and raw transcripts. The human makes the decision.
- Then delete the scratch code, or promote the idea by re-implementing it in the right crate with tests. Never leave half-wired spike code inside a crate.

<!-- Adapted from yet-more-skills/bounded-prototype (MIT OR Apache-2.0) -->
