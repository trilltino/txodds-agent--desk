# proof-guard-agent — the real, independent CoralOS participant for the
# wager-consistency proof gate. coral-server launches this container itself
# (via the Docker socket, see docker-compose.coralos.yml) and injects
# CORAL_CONNECTION_URL; the binary blocks on its own wait-for-mention loop
# from there (crates/coral-client).
#
# Build from the repo root so the workspace (Cargo.toml/Cargo.lock + every
# member Cargo.toml, needed for workspace resolution even though only
# proof-guard-agent's own dependency graph is actually compiled) is in the
# build context:
#   docker build -f crates/agents/proof-guard-agent/Dockerfile -t proof-guard-agent:0.1.0 .

FROM rust:1-slim-bookworm AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY native ./native

RUN cargo build --release -p proof-guard-agent

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/proof-guard-agent /usr/local/bin/proof-guard-agent

ENTRYPOINT ["/usr/local/bin/proof-guard-agent"]
