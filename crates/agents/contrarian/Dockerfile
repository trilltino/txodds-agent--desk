# contrarian — hybrid FADE-sharp arena agent + CoralOS bus participant.
# coral-server launches this container itself (via the Docker socket, see
# docker-compose.coralos.yml) and injects CORAL_CONNECTION_URL; the binary
# runs its TxLINE poll loop and services FADE_STATUS_REQUESTED mentions
# concurrently (crates/coral-client).
#
# Build from the repo root so the workspace is in the build context:
#   docker build -f crates/agents/contrarian/Dockerfile -t contrarian:0.1.0 .

FROM rust:1-slim-bookworm AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY native ./native

RUN cargo build --release -p contrarian

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/contrarian /usr/local/bin/contrarian

ENTRYPOINT ["/usr/local/bin/contrarian"]
