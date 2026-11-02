#!/usr/bin/env bash
# One clean take: restart the keyless demo server (`make demo`) on a fresh state folder, then run
# the driver. Replays tick every 100 ms (MM_REPLAY_PACE_MS; the server default is 250) so the
# two-week home replay fits its scene; the recorded events are unchanged.
# Usage (from the repo root):  bash scripts/demo/take.sh [driver options, e.g. --no-fault or --auto]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(pwd)"
[ -f "$repo/docs/demo-script.md" ] || { echo "run from the repo root"; exit 2; }
# runs/ is gitignored: each take's server state and log stay local.
state="$repo/runs/demo-takes/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$state"

# Stop whatever mm-server holds :8080 (only an mm-server; anything else is left alone).
pid="$(lsof -t -nP -iTCP:8080 -sTCP:LISTEN 2>/dev/null || true)"
if [ -n "$pid" ]; then
  if ps -o comm= -p "$pid" | grep -q mm-server; then kill "$pid"; sleep 1; else
    echo "port 8080 is held by something else ($(ps -o comm= -p "$pid")); stop it first"; exit 1; fi
fi

echo "starting the demo server (fresh state)…"
MM_REPLAY_PACE_MS="${MM_REPLAY_PACE_MS:-100}" MM_RUNS_DIR="$state" make -s demo > "$state/server.log" 2>&1 &
for _ in $(seq 1 240); do
  curl -s -m 1 http://127.0.0.1:8080/api/health >/dev/null 2>&1 && break
  sleep 0.5
done
curl -s -m 2 http://127.0.0.1:8080/api/health >/dev/null || { echo "server did not start; see $state/server.log"; exit 1; }
echo "server up."
node "$here/demo-drive.mjs" "$@"
