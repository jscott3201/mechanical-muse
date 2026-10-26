#!/bin/sh
# Stop hook (Muse Code and Claude Code): if Rust or web files changed this turn, run the project
# checks. Exit 2 with a report on stderr sends the agent back to work until the checks pass.
# Capped at 8 continuations per turn (Muse also caps; Claude Code relies on this counter).
# Hooks run outside the agent sandbox; PATH and HOME are kept.
payload=$(cat)
sid=$(printf '%s' "$payload" | sed -n 's/.*"session_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)
sid=${sid:-${MUSE_SESSION_ID:-local}}
root=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
cd "$root" || exit 0
tmp="${TMPDIR:-/tmp}"
stamp="$tmp/mm-turn-$sid"
count_file="$tmp/mm-stops-$sid"
if [ -f "$stamp" ]; then
  changed=$(find crates web/src web/package.json Cargo.toml Cargo.lock -type f -newer "$stamp" 2>/dev/null)
else
  changed=$(git status --porcelain | cut -c4-)
fi
[ -z "$changed" ] && exit 0
count=$(cat "$count_file" 2>/dev/null || echo 0)
if [ "$count" -ge 8 ]; then
  echo "verify.sh: 8 failed check rounds this turn; stopping so a human can look." >&2
  exit 0
fi
rpt="$tmp/mm-verify-$$.txt"; : > "$rpt"; bad=0
check() {  # check <label> <command...>
  label=$1; shift
  if ! "$@" >"$rpt.step" 2>&1; then
    bad=1; { echo "## $label failed: $*"; tail -n 40 "$rpt.step"; echo; } >>"$rpt"
  fi
  rm -f "$rpt.step"
}
if [ -f Cargo.toml ] && printf '%s\n' "$changed" | grep -Eq '\.rs$|Cargo\.(toml|lock)$'; then
  check "rustfmt" cargo fmt --all -- --check
  check "clippy"  cargo clippy --workspace --all-targets -- -D warnings
  check "tests"   cargo test --workspace
fi
if [ -f web/package.json ] && printf '%s\n' "$changed" | grep -Eq '^(\./)?web/'; then
  # TODO(decision: package manager): adjust to the scripts defined in web/package.json.
  check "web typecheck" npm --prefix web run typecheck
  check "web test"      npm --prefix web run test --if-present
fi
if [ "$bad" -eq 1 ]; then
  echo $((count + 1)) > "$count_file"
  echo "Checks failed; fix these before finishing:" >&2
  tail -c 12000 "$rpt" >&2
  rm -f "$rpt"; exit 2
fi
rm -f "$rpt" "$count_file"; exit 0
