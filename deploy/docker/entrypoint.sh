#!/bin/sh
# Starts mm-server in the container. A Model API key is optional and only ever arrives as a runtime
# environment variable (an env file passed to `docker compose`, or a Modal secret); the repo's own
# .env names it META_API, so accept that name too. The key is never printed.
set -eu
if [ -z "${MODEL_API_KEY:-}" ] && [ -n "${META_API:-}" ]; then
  MODEL_API_KEY="$META_API"
  export MODEL_API_KEY
fi
unset META_API
exec /usr/local/bin/mm-server "$@"
