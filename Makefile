# Common commands. `make check` is the fast local gate; `make ci` runs what CI runs
# (.github/workflows/ci.yml). Web targets arrive with web/.
.PHONY: check ci fmt fmt-check clippy test doctest specs types lint deny test-live live bench env-keys run fetch boptest-up boptest-down server

check: fmt-check clippy test

ci: lint clippy types doctest specs deny

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
boptest-up:
	docker compose up -d web worker provision

boptest-down:
	docker compose down

# The API on MM_ADDR (default 127.0.0.1:8080), serving specs/.
server:
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
