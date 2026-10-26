#!/usr/bin/env bash
# No-secret scan over tracked files (AGENTS.md hard rule 4: the repo is public). Matches are
# reported by a hashed path id and line number only, never echoed.
set -euo pipefail

violations=0

scan() {
  local pattern="$1" label="$2" path line matched count=0
  if git grep -qE -e "$pattern" -- ':(exclude)scripts/ci/check-no-secrets.sh' 2>/dev/null; then
    echo "FAIL: $label in tracked files:"
    while IFS= read -r -d '' path && IFS= read -r -d '' line && IFS= read -r matched; do
      [ "$count" -ge 20 ] && { echo "  (more omitted)"; break; }
      printf '  path-id=%s:%s: [REDACTED]\n' "$(printf '%s' "$path" | git hash-object --stdin)" "$line"
      count=$((count + 1))
    done < <(git grep --text -n -z -E -e "$pattern" -- ':(exclude)scripts/ci/check-no-secrets.sh' || true)
    violations=$((violations + 1))
  fi
}

scan 'LLM_[A-Za-z0-9_-]{16,}' 'Meta Model API key'
scan 'AKIA[0-9A-Z]{16}' 'AWS access key id'
scan '-----BEGIN (RSA|EC|OPENSSH|PGP) PRIVATE KEY-----' 'private key block'
scan 'gh[pousr]_[A-Za-z0-9]{36,}' 'GitHub token'
scan 'sk-[A-Za-z0-9_-]{20,}' 'API token (sk- prefix)'
scan '(/Users/|/home/)[A-Za-z0-9._-]+/' 'absolute local path'

# Only the empty template may be tracked; real .env files must never be.
if git ls-files | grep -E '(^|/)\.env($|\.)' | grep -vqx '.env.example'; then
  echo "FAIL: a .env file other than .env.example is tracked"
  violations=$((violations + 1))
fi
if git grep -qE '^[A-Z_]+=.+' -- .env.example 2>/dev/null \
  && git grep -nE '^[A-Z_]*KEY=.+' -- .env.example >/dev/null; then
  echo "FAIL: .env.example sets a key value; keep it empty"
  violations=$((violations + 1))
fi

if [ "$violations" -gt 0 ]; then
  echo
  echo "Remove the secret or path, rotate any exposed credential, and rewrite history if pushed."
  exit 1
fi
echo "OK: no-secret scan clean."
