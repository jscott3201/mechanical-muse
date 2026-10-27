# CI and test habits

Adapted from rusty-bacnet's CI and kept small. The workflow is `.github/workflows/ci.yml`; `make
ci` runs the same steps locally.

## Jobs

| Job | What it checks |
| --- | --- |
| Lint | `cargo fmt --check`; Rust files at most 500 lines of code (`scripts/ci/check-file-size.sh`); no secrets, tracked `.env` files or absolute home paths (`scripts/ci/check-no-secrets.sh`); the pins file format |
| Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Test | `cargo nextest run --workspace --locked --profile ci`, then `cargo test --doc` (nextest skips doctests), then every `specs/*/reference.py`: its self-check must pass and it must regenerate the committed `vectors.json` byte for byte; the test run also regenerates the TypeScript types in `web/src/types/generated`, which must match the committed copies |
| Deny | `cargo deny --locked check`: advisories, licenses, duplicate versions, and sources (crates.io plus the two pinned git repos only) |
| Web | In `web/`, on the Node version in `.nvmrc`: `npm ci` from the lockfile (dependency install scripts stay blocked), `npm run typecheck`, `npm test` (Vitest), `npm run build` |
| CI OK | Fails unless every job above succeeded. This is the one status to require on `main`. |

Triggers: pull requests to `main`, pushes to `main`, a weekly run (advisory databases change on
their own) and manual dispatch. Only `main` saves the Rust cache; PRs restore it.

## Pins

- The Rust toolchain: `rust-toolchain.toml` (an exact release).
- CI tools: `.github/ci-pins.env`, each with the SHA-256 of its Linux release archive, which
  `scripts/ci/install-tools.sh` checks before unpacking. Update the version and digest together.
- Actions: pinned to commit SHAs with the version in a comment.

## Test habits

- `make check` before every commit (fmt, clippy, tests); `make ci` before opening a PR.
- Tests are offline. Anything that calls the Meta API or BOPTEST is `#[ignore]` with the reason
  in the attribute, and env-gated (`make live`, `make test-live`).
- nextest runs each test in its own process and flags a test after 30 s, killing it at 2 min
  (`.config/nextest.toml`), so a hang is reported by name.
- A bug fix lands with the test that would have caught it; a test should fail on the wrong
  implementation (mutation-style checks for validators, replay and specs).
- Goldens and vectors are generated, never hand-edited: `UPDATE_GOLDEN=1` for emitter goldens,
  `python3 -I specs/<name>/reference.py` for spec vectors; review the diff.
- New sequence specs pass their gate (`crates/mm-bridge/tests/specs.rs`) before any model sees
  them.

## Known exceptions

- `RUSTSEC-2024-0449` (unsafe-libyaml unmaintained) is ignored in `deny.toml`: it comes only from
  the YAML parser `mm-bench` uses on the pinned library's cards. Replacing the parser removes it.
- The Muse Code Stop hook (`.muse/hooks/verify.sh`) still runs `cargo test`; agents may not edit
  `.muse/`, so switching it to nextest is a human change.
