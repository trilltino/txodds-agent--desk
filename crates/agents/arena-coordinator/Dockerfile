# arena-coordinator — hybrid settlement authority + CoralOS bus participant.
# coral-server launches this container itself (via the Docker socket, see
# docker-compose.coralos.yml) and injects CORAL_CONNECTION_URL; the binary
# runs its settlement poll loop and services ARENA_STATUS_REQUESTED mentions
# concurrently (crates/coral-client).
#
# Settling anything requires the position logs of match-intelligence and
# contrarian — mount a shared volume at FOLLOW_LOG / FADE_LOG (see
# coral-agent.toml) or this agent runs honestly but settles nothing.
#
# Build from the repo root so the workspace is in the build context:
#   docker build -f crates/agents/arena-coordinator/Dockerfile -t arena-coordinator:0.1.0 .

FROM rust:1-slim-bookworm AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY native ./native

RUN cargo build --release -p arena-coordinator

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/arena-coordinator /usr/local/bin/arena-coordinator

ENTRYPOINT ["/usr/local/bin/arena-coordinator"]
