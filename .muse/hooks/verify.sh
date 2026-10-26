#!/bin/sh
# Stop hook: if Rust or web files changed this turn, run the project checks.
# Exit 2 with a report on stderr makes Muse keep working until the checks pass
# (capped by max_consecutive_stop_hook_continuations, default 8).
# Hooks run outside the sandbox with a cleared environment (PATH and HOME are kept).
cat >/dev/null
root=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
cd "$root" || exit 0
tmp="${TMPDIR:-/tmp}"
stamp="$tmp/muse-turn-${MUSE_SESSION_ID:-none}"
if [ -f "$stamp" ]; then
  changed=$(find crates web/src web/package.json Cargo.toml Cargo.lock -type f -newer "$stamp" 2>/dev/null)
else
  changed=$(git status --porcelain | cut -c4-)
fi
[ -z "$changed" ] && exit 0
rpt="$tmp/muse-verify-$$.txt"; : > "$rpt"; bad=0
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
  echo "Checks failed; fix these before finishing:" >&2
  tail -c 12000 "$rpt" >&2
  rm -f "$rpt"; exit 2
fi
rm -f "$rpt"; exit 0
