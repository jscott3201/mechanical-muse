---
title: Getting started
description: Run Mechanical Muse locally in recorded mode, then add a Meta Model API key and BOPTEST for live mode.
---

There is no hosted public instance: the app runs on your machine. Without a model key or
BOPTEST everything plays back from recordings, and the UI labels each one "recorded" or
"replay". Verification is always real: every spec's vectors replay through the engine.

## Recorded mode in two minutes

With Docker only:

```sh
make docker-demo        # same as: docker compose -f compose.demo.yaml up --build
```

Or with Rust (`rust-toolchain.toml`) and Node (`web/.nvmrc`):

```sh
make demo               # builds the web app and serves it from mm-server
```

Open http://localhost:8080 (Docker) or http://127.0.0.1:8080 (`make demo`). The first Docker
build takes several minutes.

Then try:

1. **Sequences** → *Chilled-water plant reset* → *Muse Spark* → *Recorded run*, then *Draft*.
   Watch a draft fail its vectors and get repaired to a green receipt.
2. **Bring your own** → *Use the sample sequence sheet*, then *Read with Muse Spark*: Spark's
   recorded reading of a photo of a sequence, its proposed scenarios and the open questions it
   lists.
3. **Runs** → replay the day 143 morning run with revision B: trends, and KPIs beside the
   baseline over the same window. The day 186 fault demo shows the fail-safe handing control
   back; the day 185 sensor-bias record shows what a fault costs.

## Live mode

| You want | You need | How |
| --- | --- | --- |
| Live Muse Spark authoring and explanations | Your own Meta Model API key | Put `MODEL_API_KEY=...` in `.env` (git ignores it), then `make demo-live` or `make docker-demo-live` |
| Live BOPTEST runs | A reachable BOPTEST v0.9.0 | Set `MM_BOPTEST_URL`. Locally: `export BOPTEST_DIR=<checkout>; make boptest-up`, then `MM_BOPTEST_URL=http://127.0.0.1:80`. On native x86 with your own Modal account: `make boptest-modal` and use the printed URL |
| The read-only BACnet mirror | `mm-server` run natively (not in Docker) | `MM_BACNET_ADDR=0.0.0.0:47808` |

The key is read only by the server, from the environment. It never reaches the browser, the
image or the logs. Without `MM_BOPTEST_URL` the header shows "BOPTEST off" and live runs are
disabled; replays still work.

BOPTEST's worker is x86-only, so on Apple silicon a local BOPTEST runs under emulation and is
slow (16 to 26 minutes to initialize the demo window). Recorded mode, or BOPTEST on Modal, avoids
that.

## Full development setup

```sh
git clone --branch v0.9.0 --depth 1 https://github.com/ibpsa/project1-boptest
export BOPTEST_DIR=<that checkout>; make boptest-up       # or: make boptest-modal
MM_BOPTEST_URL=http://127.0.0.1:80 make server            # make server-live adds Muse Spark
cd web && npm ci && npm run dev                           # the web app in dev mode
```

`make check` runs fmt, clippy and the tests (offline: no Meta API, no BOPTEST); `make ci` runs
what CI runs.

## Learn more

- [docs/packaging.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/packaging.md):
  Docker image, live overlay, troubleshooting.
- [docs/boptest-on-modal.md](https://github.com/jscott3201/mechanical-muse/blob/main/docs/boptest-on-modal.md):
  BOPTEST on native x86.
- [README](https://github.com/jscott3201/mechanical-muse#readme): the full picture.
