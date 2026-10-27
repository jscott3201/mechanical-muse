# minio and mc at the releases BOPTEST v0.9.0 names, built from source. The published binaries
# and images for these releases are no longer available, so `compose.boptest-images.yaml` and the
# Modal image build them here. Pinned by commit (the release tags point at these commits).
FROM golang:1.24.2-bookworm AS build
ARG MINIO_COMMIT=f19c534b9f457773dcd043d977433e1a71525c3b
ARG MC_COMMIT=1e78af443bf0443eb177f26421fe081f08d83dc5
RUN git init -q /src/minio && cd /src/minio \
 && git fetch -q --depth 1 https://github.com/minio/minio "$MINIO_COMMIT" && git checkout -q FETCH_HEAD \
 && CGO_ENABLED=0 go build -trimpath -o /out/minio . \
 && git init -q /src/mc && cd /src/mc \
 && git fetch -q --depth 1 https://github.com/minio/mc "$MC_COMMIT" && git checkout -q FETCH_HEAD \
 && CGO_ENABLED=0 go build -trimpath -o /out/mc .

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/minio /out/mc /usr/bin/
EXPOSE 9000 9001
ENTRYPOINT ["/usr/bin/minio"]
