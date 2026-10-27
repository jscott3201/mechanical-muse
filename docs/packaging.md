# Packaging: run it without Rust or Node

One image holds the whole app: the web app, served by `mm-server` on port 8080, plus the specs and
the recorded sessions, runs and explanations. Two ways to start it, neither needing a Rust or Node
toolchain: Docker Compose on your machine, or Modal.

## Docker Compose (recommended)

Needs Docker with Compose v2.24 or later (Docker Desktop is fine). From the repo root:

```sh
docker compose -f compose.demo.yaml up --build      # same as: make docker-demo
```

Open http://localhost:8080. The first build compiles the server and the web app inside Docker and
takes several minutes; later starts reuse the image. Stop with Ctrl-C, then:

```sh
docker compose -f compose.demo.yaml down            # same as: make docker-demo-down
```

The port is published on 127.0.0.1 only. Use another host port with `MM_PORT=9090`.

### What works with no key and no BOPTEST (recorded mode)

- Every spec loads and its test vectors replay through the engine (real verification, live in the
  container).
- The recorded Muse Spark authoring session plays back attempt by attempt, labelled "recorded".
- Recorded BOPTEST runs replay over the same event stream, labelled "replay", including the
  labelled test fault that shows the fail-safe (`day186-midnight-fault-demo`).
- Recorded plain-language explanations, labelled as model-written.

### Live mode (optional)

Live Muse Spark authoring and explanations need your own Meta Model API key. Put it in an env file
as `MODEL_API_KEY=...` (default `./.env`, which git ignores; `MM_ENV_FILE=<path>` for another) and
add the overlay:

```sh
docker compose -f compose.demo.yaml -f compose.demo.live.yaml up --build   # make docker-demo-live
```

The key reaches the container only as a run-time environment variable. It is never a build
argument, never in the image, never sent to the browser. `docker inspect` and
`docker compose config` do show container environment values, so do not share their output.
`/api/health` reports `live_model` when a key is loaded.

Live BOPTEST runs need a BOPTEST v0.9.0 the container can reach:

- On this machine: `export BOPTEST_DIR=<checkout>; make boptest-up` (see the README). The demo
  container reaches it at `http://host.docker.internal:80`, its default.
- On Modal (native x86, much faster on Apple silicon): `make boptest-modal`
  ([boptest-on-modal.md](boptest-on-modal.md)), then
  `MM_BOPTEST_URL=<printed url> docker compose -f compose.demo.yaml up`.

Without a reachable BOPTEST, starting a live run reports that BOPTEST is unreachable; everything
recorded keeps working.

The read-only BACnet mirror is not enabled in the container: BACnet/IP relies on UDP broadcast,
which Docker's default network does not carry to your LAN. Run `mm-server` natively for it
([bacnet-mirror.md](bacnet-mirror.md)).

## Modal

Needs the `modal` CLI, logged in (`modal setup`). From the repo root:

```sh
modal serve deploy/modal/mm_demo.py       # temporary URL while the command runs (make modal-demo)
modal deploy deploy/modal/mm_demo.py      # stays deployed, scales to zero when idle
modal app stop mm-demo                    # stop it
```

Modal builds the same `Dockerfile` on its own x86 builders (the first build takes several
minutes), then prints the web endpoint URL. That URL is public: anyone holding it can use the app.
It runs in recorded mode unless you set, in the shell that runs `modal serve` or `modal deploy`:

- `MM_BOPTEST_URL`: a BOPTEST for live runs, for example the tunnel URL from
  `make boptest-modal` (BOPTEST has no authentication; do not post either URL).
- `MM_MODAL_SECRET`: the name of a Modal secret holding `MODEL_API_KEY`, for live Muse Spark.
  Create it once with `modal secret create mm-model-key MODEL_API_KEY=<key>` and pass
  `MM_MODAL_SECRET=mm-model-key`. Live calls through a public URL are billed to that key.

The app runs one container (the server keeps runs in memory and drives one live run at a time).
Runs and verified drafts made there are lost when it scales down.

## What is in the image

| Stage | Base image (tag and digest) | Role |
| --- | --- | --- |
| web | `node:26.12.1-trixie-slim@sha256:36793bcbb160a6a16cede1e62b93f84e26b78a972a8a4c0b879c1c12f22761ec` | `npm ci --ignore-scripts && npm run build` (Node per `web/.nvmrc`) |
| server | `rust:1.97.1-slim-trixie@sha256:8e8cf8f7fd54a2d23d5a743b3a03f56e26b6c774276c33fa0595111704ebb15c` | `cargo build --release --locked -p mm-server` (Rust per `rust-toolchain.toml`) |
| runtime | `debian:trixie-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad` | `mm-server`, `web/dist`, `specs/`, `fixtures/{recordings,replays,explanations}`; curl for the health check |

These are ordinary third-party base images (Node.js, Rust and Debian official images), used
unmodified. The runtime runs as an unprivileged user (uid 10001); in Compose its filesystem is
read-only except a temporary `runs/`. `.dockerignore` keeps `.env` files, `runs/`, `research/`,
`scratch/`, `ledger/`, `.claude/`, `.git`, `target/` and `node_modules/` out of the build context.

## Troubleshooting

- **Apple silicon.** The app image builds natively for arm64 and for x86-64. BOPTEST's worker is
  x86-only, so a local BOPTEST runs under emulation: 16 to 26 minutes to initialize the demo
  window, and slow steps. Use `make boptest-modal` for live runs, or stay in recorded mode.
- **Port 8080 in use.** `MM_PORT=9090 docker compose -f compose.demo.yaml up`, then open
  http://localhost:9090.
- **Health.** `curl -fsS http://localhost:8080/api/health` returns the engine rev, the spec count
  and whether a live model is loaded; `docker compose -f compose.demo.yaml ps` shows the health
  check state.
- **Live run says BOPTEST is unreachable.** Check `MM_BOPTEST_URL`. From inside the container,
  `localhost` is the container itself; use `host.docker.internal` for this machine.
- **The live overlay fails with "env file not found".** Create the env file, or set `MM_ENV_FILE`.
- **Rebuild after pulling changes.** `docker compose -f compose.demo.yaml build --no-cache`
  if a cached layer looks stale.
- **Logs.** `docker compose -f compose.demo.yaml logs -f`; `RUST_LOG=debug` for more.
