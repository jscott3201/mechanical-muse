# Common commands. Rust checks match the Stop hook; web targets arrive with web/.
.PHONY: check fmt fmt-check clippy test test-live run fetch

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

run:
	cargo run -p mm-server

# Run outside the Muse sandbox; never `cargo update` the pinned git dependencies.
fetch:
	cargo fetch --locked
