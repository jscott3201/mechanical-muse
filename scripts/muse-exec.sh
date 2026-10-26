#!/bin/sh
# Headless Muse Code run. Usage: scripts/muse-exec.sh "<prompt>"
# Needs META_API_KEY in the environment. --disable-approval keeps the sandbox on.
# --trust-workspace loads AGENTS.md, skills and hooks: only use it on this checkout.
# Exit 0 means the turn completed, not that the work is correct; the Stop hook runs the checks.
exec muse exec --json --disable-approval --trust-workspace --contributor-mode off \
  --model muse-spark-1.3 --reasoning-effort medium --max-model-steps 80 \
  -c agents.execution_capacity=4 "$@"
