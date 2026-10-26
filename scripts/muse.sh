#!/bin/sh
# Interactive Muse Code launcher for this repo. Pins the model and keeps the sandbox on.
# Muse Code's default model is muse-spark-1.2, so always pass --model.
# Contributor mode off: session content is not used for training.
exec muse --model muse-spark-1.3 --reasoning-effort high \
  --sandbox-network proxy-only --contributor-mode off \
  -c agents.execution_capacity=4 "$@"
