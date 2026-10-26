#!/bin/sh
# UserPromptSubmit hook (Muse Code and Claude Code): mark the start of a turn so verify.sh only
# checks files changed this turn. Reads the hook payload from stdin to find the session id.
payload=$(cat)
sid=$(printf '%s' "$payload" | sed -n 's/.*"session_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)
sid=${sid:-${MUSE_SESSION_ID:-local}}
tmp="${TMPDIR:-/tmp}"
touch "$tmp/mm-turn-$sid"
rm -f "$tmp/mm-stops-$sid"
exit 0
