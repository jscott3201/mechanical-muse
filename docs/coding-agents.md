# Coding agents: Muse Code and Claude Code

How this repo is set up for the two coding harnesses we switch between during the build.
Agent-facing rules live in `AGENTS.md`; this page is for the human driving the sessions. Any tool
may build the repo (the organizers confirmed this on 2026-10-26), but every model call the
application makes goes to Meta's models: Muse Spark through the Model API, Glimmer on Modal
(`AGENTS.md` hard rule 8). Judges score the product's use of Meta's models.

## What both harnesses share

| Piece | File | Notes |
|---|---|---|
| Context | `AGENTS.md` | Claude Code reads it natively only while no `CLAUDE.md` exists anywhere above the working directory (v2.1.277+). Muse Code reads it directly. |
| Skills | `.agents/skills/<id>/SKILL.md` | The committed copy (`mm-*` only), read by Muse Code. General-purpose skills from before the event are kept locally and gitignored. Claude Code reads only `.claude/skills/`, which is local; keep it in sync with `.agents/skills/` (copy or symlink). Muse also scans `.claude/skills/` and may flag the same id in both places as a conflict. |
| Verification | `.muse/hooks/stamp.sh`, `.muse/hooks/verify.sh` | Wired by `.muse/hooks.json` (Muse) and the local `.claude/settings.json` (Claude Code). Fmt, clippy, tests and web checks for files changed this turn; up to 8 rounds. |
| Ledger | `ledger/HANDOFF.md`, `ledger/OPEN.md` | Local only (gitignored). Each session reads both at the start and appends a dated handoff entry at the end, so the next session, in either tool, picks up where the last stopped. |
| MCP | `.mcp.json` | shadcn server (`npx shadcn@latest mcp`) and `mechanical-muse` (`mm-mcp`: our spec, palette, validate, emit and verify tools, `docs/mcp.md`). Muse loads it after trust; Claude Code asks once, or uses `enabledMcpjsonServers` in `.claude/settings.json` once trusted. |

Commit the shared pieces before spawning isolated workers in either tool: worktrees start from HEAD.
Gitignored folders (`.claude/`, `ledger/`) are not in a worktree, so Claude Code workers there run
without the Stop hook or skills; run the checks yourself before merging their work.

## Muse Code

- Install from https://dev.meta.ai/docs/muse-code; sign in or set `META_API_KEY` (the product
  itself uses `MODEL_API_KEY`).
- Launch with `scripts/muse.sh`: pins `muse-spark-1.3` (Muse defaults to 1.2), keeps the sandbox
  on, turns contributor mode off, caps parallel agents at 4. Headless: `scripts/muse-exec.sh "<prompt>"`.
- Optional `~/.config/muse/settings.json` (Muse rewrites it; keep a copy). `schema_version` is required:

  ```json
  {
    "schema_version": 1,
    "permissions": { "default_profile": ":auto-review" },
    "agents": { "execution_capacity": 4 },
    "context": { "foreign_personal_skills": false }
  }
  ```

- The sandbox makes `$HOME` and `.git` read-only and proxies network per host: run `cargo fetch`
  and the web install yourself first. Loopback ports (BOPTEST on 80) are governed by
  `network.local_ports`. `.claude/` is not sandbox-protected in Muse, so `AGENTS.md` tells agents
  not to edit it.
- Useful: `/goal` with a test command as the finish line, `/plan` (saves to `.agents/plans/`),
  `/grill-with-docs` (writes `docs/DECISIONS.md`), `/export trajectory --redacted`.
- Skills: `muse skills validate .claude/skills/<id> --json`, `muse skills list --source project`,
  `/skills diagnostics`, `/skills reload` after edits.
- Workflows (`.agents/workflows/*.js`) are not available on the Apple-silicon public build.

## Claude Code

- `.claude/` is local for now (gitignored). Its `settings.json` wires the Stop hook, denies edits
  to `.claude/` and `.muse/`, denies force pushes, `cargo publish` and `cargo update`, and enables
  the shadcn MCP server. Plain pushes and commits run without a prompt; agents still commit or push
  only when the human asks in that session. Personal overrides go in
  `.claude/settings.local.json`.
- Model: use Claude models for development, or run Claude Code on Meta's models through the Model
  API's Messages endpoint (from Meta's coding-agents guide):

  ```sh
  export ANTHROPIC_BASE_URL=https://api.meta.ai          # no /v1 suffix
  export ANTHROPIC_AUTH_TOKEN="$MODEL_API_KEY"           # bearer token, not ANTHROPIC_API_KEY
  export ANTHROPIC_MODEL=muse-wonder-1.0
  export ANTHROPIC_DEFAULT_OPUS_MODEL=muse-wonder-1.0
  export ANTHROPIC_DEFAULT_SONNET_MODEL=muse-spark-1.3
  export ANTHROPIC_DEFAULT_HAIKU_MODEL=muse-spark-1.3
  export CLAUDE_CODE_SUBAGENT_MODEL=muse-spark-1.3
  export ENABLE_TOOL_SEARCH=true
  ```

  Keep these in your shell profile or a local launcher, never in a committed file. The Messages
  endpoint rejects some request shapes (thinking disabled, stop sequences, named tool choice), and
  prompt caching is undocumented there, so smoke-test a short session and watch spend.
- Skills appear in `/skills`; MCP status in `/mcp`; settings sources in `/status`.

## Habits for either harness

- Name the finish line in the prompt ("done when `cargo test --workspace` passes and the new golden
  test exists"). The Stop hook enforces the checks; it does not judge whether the work is right.
- Commit from your own terminal in small, labelled commits. Note in the message which harness and
  model produced a change if it helps the provenance story.
- Writers in parallel get isolated worktrees; read-only explorers share the checkout; merge one
  child at a time.
- Keep skill descriptions as triggers of 200 characters or fewer with a
  `metadata.short-description`; Muse compresses its skill catalog past 24 KB.
