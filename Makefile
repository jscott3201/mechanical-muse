# Common commands. Rust checks match the Stop hook; web targets arrive with web/.
.PHONY: check fmt fmt-check clippy test test-live live bench env-keys run fetch

check: fmt-check clippy test

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

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
