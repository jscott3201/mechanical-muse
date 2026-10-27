# Mechanical Muse in one image: the web app served by mm-server on port 8080, with the specs and
# the recorded sessions, runs and explanations, so it works with no model key and no BOPTEST
# (everything recorded is labelled in the UI). See docs/packaging.md.
#
#   docker build -t mechanical-muse .        # or: make docker-demo
#
# Base images are pinned by tag and digest (docs/packaging.md lists them). The model key is never
# a build argument: it reaches the server only as a runtime environment variable.

# --- web app (Node per web/.nvmrc) ---
FROM node:26.12.1-trixie-slim@sha256:36793bcbb160a6a16cede1e62b93f84e26b78a972a8a4c0b879c1c12f22761ec AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --ignore-scripts --no-audit --no-fund
COPY web/ ./
RUN npm run build

# --- mm-server (Rust per rust-toolchain.toml) ---
FROM rust:1.97.1-slim-trixie@sha256:8e8cf8f7fd54a2d23d5a743b3a03f56e26b6c774276c33fa0595111704ebb15c AS server
# The image's toolchain is the pinned channel; this stops rustup from fetching the extra
# components rust-toolchain.toml lists (rustfmt, clippy), which a release build does not need.
ENV RUSTUP_TOOLCHAIN=1.97.1 CARGO_TERM_COLOR=never
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
# The whole workspace (minus .dockerignore): crates embed files from docs/ at compile time.
COPY . .
RUN cargo build --release --locked -p mm-server \
 && cp target/release/mm-server /mm-server

# --- runtime ---
FROM debian:trixie-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad
# curl only for the health check; TLS to the Model API uses rustls with bundled roots.
RUN apt-get update && apt-get install -y --no-install-recommends curl \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 10001 muse \
 && useradd --system --uid 10001 --gid muse --home-dir /app --shell /usr/sbin/nologin muse
WORKDIR /app
COPY --from=server /mm-server /usr/local/bin/mm-server
COPY --from=web /src/web/dist ./web
COPY specs ./specs
COPY fixtures/recordings ./fixtures/recordings
COPY fixtures/replays ./fixtures/replays
COPY fixtures/explanations ./fixtures/explanations
COPY deploy/docker/entrypoint.sh /usr/local/bin/mm-entrypoint
RUN mkdir -p runs && chown muse:muse runs && chmod 0755 /usr/local/bin/mm-entrypoint
USER muse
ENV MM_ADDR=0.0.0.0:8080 \
    MM_WEB_DIR=/app/web \
    MM_SPECS_DIR=/app/specs \
    MM_RUNS_DIR=/app/runs \
    MM_RECORDINGS_DIR=/app/fixtures/recordings \
    MM_REPLAYS_DIR=/app/fixtures/replays \
    MM_EXPLANATIONS_DIR=/app/fixtures/explanations
EXPOSE 8080
# mm-server stops cleanly (closing streams, stopping an active BOPTEST test) on SIGINT.
STOPSIGNAL SIGINT
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8080/api/health > /dev/null || exit 1
ENTRYPOINT ["/usr/local/bin/mm-entrypoint"]
