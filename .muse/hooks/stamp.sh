#!/bin/sh
# UserPromptSubmit hook: mark the start of a turn so verify.sh only checks files changed this turn.
cat >/dev/null
touch "${TMPDIR:-/tmp}/muse-turn-${MUSE_SESSION_ID:-none}"
exit 0
