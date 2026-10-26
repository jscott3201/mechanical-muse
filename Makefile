# Common commands. Rust checks match the Stop hook; web targets arrive with web/.
.PHONY: check fmt fmt-check clippy test test-live live env-keys run fetch

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

# Names of the variables in .env, each marked set or empty. Never prints values.
env-keys:
	@sh scripts/env-keys.sh

run:
	cargo run -p mm-server

# Run outside the Muse sandbox; never `cargo update` the pinned git dependencies.
fetch:
	cargo fetch --locked
