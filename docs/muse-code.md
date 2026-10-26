# Working on Mechanical Muse with Muse Code

Setup and habits for building this repo with Meta's Muse Code CLI. Agent-facing rules live in
`AGENTS.md`; this page is for the human driving the sessions. Facts come from the Muse Code docs
at https://dev.meta.ai/docs/muse-code (checked Oct 26, 2026); re-check with `muse --help` after
installing, because the docs and changelog disagree in places.

## One-time setup

1. Install Muse Code from https://dev.meta.ai/docs/muse-code and sign in (browser sign-in or
   `META_API_KEY`). The product itself uses `MODEL_API_KEY`; Muse Code reads `META_API_KEY`.
2. Optional user settings at `~/.config/muse/settings.json` (Muse rewrites this file, so keep a copy):

   ```json
   {
     "schema_version": 1,
     "permissions": { "default_profile": ":auto-review" },
     "agents": { "execution_capacity": 4 },
     "context": { "foreign_personal_skills": false }
   }
   ```

   `schema_version` is required or every command fails. `foreign_personal_skills: false` keeps a
   personal `~/.claude/skills` or `~/.codex/skills` library from crowding the 24 KB skill catalog.
3. Commit `AGENTS.md`, `.agents/`, `.muse/hooks.json`, `.muse/hooks/`, and `.mcp.json` before the
   first session that spawns isolated workers: worktrees are created from HEAD.

## Before each working session

- Pre-fetch dependencies in your own terminal: `cargo fetch` and the web install. The sandbox makes
  `$HOME` read-only and sends network through a proxy that asks once per host.
- Start BOPTEST if the task needs it (`docker compose up -d`). Loopback ports are governed by
  `network.local_ports` in a permission profile, not `network.targets`.
- Launch with `scripts/muse.sh`. It pins `muse-spark-1.3` (Muse Code defaults to 1.2), keeps the
  sandbox on, turns contributor mode off, and caps parallel agents at 4.
- Trust the workspace when asked; skills, hooks, and `.mcp.json` load only after that.

## Habits that pay off

- Name the finish line in the prompt or with `/goal`, for example "done when
  `cargo test --workspace` passes and the new golden test exists".
- The Stop hook (`.muse/hooks/verify.sh`) runs fmt, clippy, and tests for changed Rust files and
  the web checks for changed `web/` files. A failure sends the agent back to work, up to 8 times per
  turn. To test it, break a test on purpose once and watch the agent fix it.
- Commit from your own terminal in small, labelled commits; agents cannot write `.git`. Export a
  session with `/export trajectory --redacted` when a transcript would help the provenance story.
- Use `/plan` for non-trivial work (plans land in `.agents/plans/`) and `/grill-with-docs` to
  record decisions in `docs/DECISIONS.md`.
- Ask for `worktree_isolation: true` on subagents that write; keep read-only explorers in the main
  checkout. Merge children one at a time.
- Headless runs: `scripts/muse-exec.sh "<prompt>"` (needs `META_API_KEY`). Exit 0 only means the
  turn finished; trust the Stop hook and your own checks.

## Skills

Project skills live in `.agents/skills/<id>/SKILL.md`. The `mm-*` skills are written for this
repo; the others are trimmed forks of `yet-more-skills` (MIT OR Apache-2.0, same author).

- Validate: `muse skills validate .agents/skills/<id> --json`
- List what loaded: `muse skills list --source project`, or `/skills` and `/skills diagnostics`
  in a session. After editing, `/skills reload`.
- Keep each description a trigger ("Use when ...") of 200 characters or fewer, and give every
  skill a `metadata.short-description`; the catalog is compressed past 24 KB.
- Do not reuse built-in ids (`plan`, `grill`, `grilling`, `taste`, `git`, `doctor`, `migrate`,
  `create-skill`, `python-env`), and do not keep the same id in `.agents/skills` and
  `.claude/skills`.

## MCP

`.mcp.json` registers the shadcn MCP server (`npx shadcn@latest mcp`) as optional. MCP servers run
outside the sandbox; restart Muse after editing `.mcp.json`. Once `web/` exists, consider pinning
the shadcn version from `web/package.json` instead of `@latest`.

## Known gaps

- Workflows (`.agents/workflows/*.js`) are not available on the Apple-silicon public build.
- No documented project settings file, custom subagent file format, or `.museignore`.
- How an in-session `git commit` is permitted is undocumented; we do not rely on it.
