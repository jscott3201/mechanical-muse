# BOPTEST on Modal

BOPTEST's worker is an x86 image. On Apple silicon it runs under emulation, which makes a run
slow to start (16 to 26 minutes to initialize our demo window) and slow to step. Running BOPTEST
on [Modal](https://modal.com) instead puts it on native x86 hardware: in our measurements
initialize took 170 s and each 120 s control step 0.24 s, tunnel round trip included, with
results identical to the local run (`docs/runtime-results.md`). It is the faster way to try the
live loop, and an alternative to the local Docker setup for anyone reviewing the project.

BOPTEST is pre-existing work by IBPSA (see the README's dependency table); this only packages
its v0.9.0 services into one container. Modal is a hosting service; nothing here depends on it
beyond `deploy/modal/`.

## What it runs

`deploy/modal/mm_boptest.py` builds one x86 image from a BOPTEST v0.9.0 checkout
(`deploy/modal/Dockerfile`: BOPTEST's worker and web build steps, redis, and minio built from
source at the release BOPTEST names, because its published binaries and images are gone), starts
the four services on localhost (`deploy/modal/start.sh`), uploads the
`multizone_office_complex_air` test case, and opens a Modal tunnel to BOPTEST's web service.
Size: 4 cores, 16 GiB. At Modal's list prices that is about $0.32 an hour; the app stops itself
after `MINUTES` (default 120).

## Run it

You need the `modal` CLI, logged in (`modal setup`), and a BOPTEST checkout:

```sh
git clone --branch v0.9.0 --depth 1 https://github.com/ibpsa/project1-boptest <dir>
export BOPTEST_DIR=<dir>
make boptest-modal            # MINUTES=60 make boptest-modal for a shorter session
```

The first run builds the image (about 5 minutes; later runs reuse it). When it prints
`BOPTEST_URL https://...`, start the server against it in another terminal:

```sh
MM_BOPTEST_URL=<that url> make server     # or make server-live for Muse Spark authoring
cd web && npm run dev
```

Then start a run from the web app. Stop the app when you are done:

```sh
modal app stop mm-boptest
```

Stopping the local `make boptest-modal` process does not always stop the app on Modal; the
command above does (`modal app list` shows what is running).

## Notes

- The tunnel URL is not guessable, but BOPTEST has no authentication: anyone holding the URL can
  use it while the app runs. Do not post it.
- One BOPTEST worker, one test at a time, as locally: `mm-server` runs one live run at a time.
- An idle BOPTEST test stops after 900 s (BOPTEST's own setting); start a run soon after the
  first `select`.
