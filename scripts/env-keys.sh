#!/bin/sh
# Lists the variable names in the local .env file, each marked "set" or "empty".
# Never prints a value. Agents use this instead of reading .env directly.
f="$(git rev-parse --show-toplevel)/.env"
[ -f "$f" ] || { echo "no .env file"; exit 0; }
grep -E '^[A-Za-z_][A-Za-z0-9_]*=' "$f" | while IFS='=' read -r name value; do
  if [ -n "$value" ]; then echo "$name: set"; else echo "$name: empty"; fi
done
