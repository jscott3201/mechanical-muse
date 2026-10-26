---
name: parallel-lanes
description: "Guides splitting work across parallel Muse Code subagents: crate lanes, worktree isolation, shared surfaces, one-at-a-time merging. Use only when asked to delegate parallel work."
license: MIT OR Apache-2.0
metadata:
  short-description: "Split work across parallel Muse Code subagents"
---
# Parallel Lanes

Lanes cost extra tokens and integration effort, so use them only for independent outcomes, with at most 4 concurrent subagents (the Muse default is 8). When the work does not separate cleanly, stay in one lane.

## Plan the lanes
- Give each lane one outcome, its owned paths, the checks to run and the interfaces it may assume. Natural lanes are the crates: `mm-graph`, `mm-bridge`, `mm-agent`, `mm-runtime`, `mm-bacnet`, `mm-server`, plus `web/`, `specs/` and `fixtures/`.
- The parent owns shared surfaces: root `Cargo.toml` and `Cargo.lock`, the DTO-to-generated-TS step, goldens, `docs/upstream-gaps.md`, the README provenance table, `compose.yaml` and `AGENTS.md`. Workers propose changes there in their return note.
- A lane never guesses another lane's unfinished contract. Land a small shared foundation first, or have the parent define the trait or DTO stub.

## Run them in Muse Code
- Writers: `subagent_spawn` with `worktree_isolation: true`. Children that only read (survey, review, research) may share the checkout.
- A worktree starts from HEAD, so the human must commit `AGENTS.md`, `.agents/`, config and the shared foundation before spawning. Uncommitted edits in the parent checkout are invisible to children.
- Each worktree is cold: no `target/` and no `node_modules/`. Expect a full Rust build and a fresh web install, and do not start several heavy builds at once.
- Some resources allow one user at a time: a single BOPTEST container (one writer), BACnet UDP 47808, and the per-team Meta rate limits (one lane runs live model calls; others use fixtures).
- Agents never commit; `.git` is read-only and the human commits. Each child returns its changed-files list, the commands it ran with real results, and its interface assumptions. Brief it with its lane's context only, not whole transcripts.

## Merge
- The parent merges children one at a time: review the child's diff, apply it to the main checkout, run fmt, clippy and `cargo test --workspace`, then take the next. Discard a rejected child by removing its worktree; leave worktrees holding unmerged work alone.
- Test the combined path after each merge, not each lane in isolation, and revisit lanes whose assumptions changed. On a contract conflict, stop and ask the human instead of inventing the design at merge time.

<!-- Adapted from yet-more-skills/parallel-portfolio (MIT OR Apache-2.0) -->
