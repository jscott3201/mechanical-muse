---
name: mm-hackathon-boundary
description: "Guards the hackathon new-work boundary, README provenance tables, claim wording and public-repo hygiene. Use when editing README, UI copy, demo text, commits, dependencies, datasets or results."
license: MIT OR Apache-2.0
metadata:
  short-description: "New-work boundary, provenance, honest claims"
---
Event 2026-10-26 to 2026-11-04; this repo started 2026-10-26. Official Rules, paraphrased: open-source software may be used if its licenses are followed and the entry builds on it, but the entry must be original, solely owned work; the repo is public; the demo may show only event-built work, clearly identifiable, or the entry is disqualified.

**Pre-existing** (pinned, unmodified, dated before the event, our own earlier repos included)
- open-control-engine v0.2.0 `4063202b`; open-control-library `124427b` (data only); rusty-bacnet v0.14.0 `1caf4db7`; BOPTEST by IBPSA, an external service (README pin is TBD: fill it from the commit `compose.yaml` builds); `yet-more-skills` `97d2a36` (source of the non-`mm-*` skill forks, notices kept); ordinary crates and npm packages.
- New work: `crates/`, `web/`, `specs/`, `docs/`, `fixtures/`, `.agents/skills/mm-*`.

**Same-change rules**
- Add or adjust the README "Pre-existing dependencies" row (name, author, license, pinned commit, use) in the same change as any new dependency, dataset, shadcn component, extra UI library, copied skill or ported Modelica file. Modelica Buildings Library (BSD-3) lives only under `third_party/` with its notice.
- Update the README "Built during the hackathon" status and the AGENTS.md Status line when a component lands.
- In README, UI copy, demo script and commit messages say "uses" or "calls" for engine, library, BACnet-stack and BOPTEST behavior; claim only the bridge, emitter, agent, runtime, mirror, web app, harness and specs.

**Text and claims**
- No ASHRAE Guideline 36, 231P/CXF or 135 text, tables or figures: describe behavior in our own words ("G36-style sequence"). Imply no endorsement by ASHRAE, LBNL, IBPSA or Meta.
- Approved novelty line: "To our knowledge, the first open benchmark that scores LLM-authored CDL by replaying test vectors through a deterministic engine, then runs verified sequences closed-loop on a building emulator."
- Avoid unqualified "first", "verifiably correct/proven/safe", "G36 compliant", pass-rate comparisons with prior papers (Wan et al.: up to 83%), "field-tested". Rewrites and a no-"first" variant: `references/claims.md`.
- Results read "passes X of Y library test vectors" with the baseline beside them; "spec-derived" only if literally true.

**Hygiene** (public repo, including `.agents/`, `.muse/`, docs, prompts, fixtures)
- No keys, tokens, `.env`, absolute paths, personal emails, machine or session ids, or transcripts that echo credentials; `.env.example` stays empty. Gitignored local notes (`scratch/` and similar) are never linked from committed files.
- The three-minute video shows only event-built work and says in its first 15 seconds what is new vs reused (`mm-demo-prep`).
