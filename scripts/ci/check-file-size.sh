#!/usr/bin/env bash
# Size cap for Rust files (AGENTS.md: "Rust files stay under roughly 500 lines; split by
# responsibility"). Counts non-empty, non-comment lines in tracked .rs files.
set -euo pipefail

CAP=500
violations=0
while IFS= read -r f; do
  [ -f "$f" ] || continue
  loc=$(grep -cvE '^\s*(//.*)?$' "$f" || true)
  if [ "$loc" -gt "$CAP" ]; then
    echo "FAIL: $f has $loc lines of code (cap $CAP)"
    violations=$((violations + 1))
  fi
done < <(git ls-files '*.rs')

if [ "$violations" -gt 0 ]; then
  echo "Split files over the cap by responsibility."
  exit 1
fi
echo "OK: every tracked .rs file is within $CAP lines of code."
