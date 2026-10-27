# Common commands. `make check` is the fast local gate; `make ci` runs what CI runs
# (.github/workflows/ci.yml).
.PHONY: web check ci fmt fmt-check clippy test doctest specs types lint deny test-live live bench env-keys run fetch boptest-up boptest-down boptest-minio boptest-modal server server-live web-build demo demo-live

check: fmt-check clippy test

ci: lint clippy types doctest specs deny web

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

# nextest when installed (each test in its own process, see .config/nextest.toml), else cargo test.
test:
	@if cargo nextest --version >/dev/null 2>&1; then \
		cargo nextest run --workspace --locked; \
	else \
		cargo test --workspace --locked; \
	fi

# nextest does not run doctests.
doctest:
	cargo test --doc --workspace --locked

# Each spec's reference model self-checks and must regenerate its committed vectors exactly.
specs:
	@for ref in specs/*/reference.py; do [ -f "$$ref" ] || continue; python3 -I "$$ref" || exit 1; done
	git diff --exit-code -- specs/

# ts-rs writes the TypeScript types during the test run; the committed copies must match.
# Generated files are removed first, so a renamed or deleted DTO cannot leave an orphan behind.
types:
	rm -f web/src/types/generated/*.ts
	$(MAKE) -s test
	@test -z "$$(git status --porcelain -- web/src/types/generated)" || \
		{ git status --porcelain -- web/src/types/generated; echo "regenerate and commit web/src/types/generated"; exit 1; }

lint: fmt-check
	bash scripts/ci/check-file-size.sh
	bash scripts/ci/check-no-secrets.sh

deny:
	cargo deny --locked check

# BOPTEST v0.9.0 on http://127.0.0.1:80 (compose.yaml; both targets need BOPTEST_DIR). provision
# uploads the test cases once and exits; the first run builds the images (after switching
# BOPTEST versions, rebuild with `docker compose build`).
boptest-up: boptest-minio
	docker compose up -d web worker provision

# minio and mc built from source at the releases BOPTEST names (deploy/minio/Dockerfile).
boptest-minio:
	docker build -q -t mm-boptest-minio:RELEASE.2025-04-22T22-12-26Z deploy/minio

# BOPTEST on Modal (native x86, several times faster than emulation): builds the image on first
# use and prints a tunnel URL for MM_BOPTEST_URL. Needs the modal CLI and a BOPTEST checkout.
# Stops after MINUTES (default 120); `modal app stop mm-boptest` stops it early.
MINUTES ?= 120
boptest-modal:
	BOPTEST_SRC="$${BOPTEST_DIR:?set BOPTEST_DIR to a BOPTEST v0.9.0 checkout}" \
	modal run deploy/modal/mm_boptest.py --minutes $(MINUTES)

boptest-down:
	docker compose down

# The web app: install from the lockfile, typecheck, test, build (Node per web/.nvmrc).
web:
	cd web && npm ci --no-audit && npm run typecheck && npm test && npm run build

# The API on MM_ADDR (default 127.0.0.1:8080), serving specs/.
server:
	cargo run -p mm-server

# One command, one address (http://127.0.0.1:8080): builds the web app and serves it from
# mm-server. Without a key or BOPTEST it still verifies specs and plays back recorded sessions,
# runs and explanations (all labelled). Set MM_BOPTEST_URL for live runs, MM_BACNET_ADDR for
# the mirror; demo-live adds live Muse Spark from the local .env key.
web-build:
	cd web && { [ -d node_modules ] || npm ci --ignore-scripts; } && npm run build

demo: web-build
	@echo "Mechanical Muse on http://127.0.0.1:8080 (Ctrl-C to stop)"
	MM_WEB_DIR=web/dist cargo run -q -p mm-server

demo-live: web-build
	@echo "Mechanical Muse on http://127.0.0.1:8080, live Muse Spark (Ctrl-C to stop)"
	@set -a; . ./.env; set +a; \
	MODEL_API_KEY="$${MODEL_API_KEY:-$$META_API}" MM_WEB_DIR=web/dist \
	cargo run -q -p mm-server

# The server with live Muse Spark authoring: maps the local .env key without printing it.
server-live:
	@set -a; . ./.env; set +a; \
	MODEL_API_KEY="$${MODEL_API_KEY:-$$META_API}" \
	cargo run -p mm-server

# Needs BOPTEST up and/or MODEL_API_KEY.
test-live:
	cargo test --workspace -- --ignored

# First live Muse Spark call (costs a few cents). Loads .env into this process only and never
# prints a value. Uses MODEL_API_KEY, falling back to META_API.
live:
	@set -a; . ./.env; set +a; \
	MODEL_API_KEY="$${MODEL_API_KEY:-$$META_API}" \
	cargo test -p mm-agent --test author_loop -- --ignored --nocapture

# Benchmark run on a library checkout at the pinned commit. Example:
#   make bench LIB=<open-control-library at 124427b> ARGS="--subset test_clean --tier b --out runs/x.json"
# Add --dry-run to scrub tasks and compute baselines without any model call.
bench:
	@set -a; . ./.env; set +a; \
	MODEL_API_KEY="$${MODEL_API_KEY:-$$META_API}" \
	cargo run --release -q -p mm-bench -- --library "$(LIB)" --library-commit 124427b $(ARGS)

# Names of the variables in .env, each marked set or empty. Never prints values.
env-keys:
	@sh scripts/env-keys.sh

run:
	cargo run -p mm-server

# Run outside the Muse sandbox; never `cargo update` the pinned git dependencies.
fetch:
	cargo fetch --locked
