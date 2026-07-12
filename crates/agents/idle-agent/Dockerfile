# idle-agent — generic idle CoralOS participant. Reused by two manifests
# (crates/agents/match-intelligence-agent-proxy, crates/agents/user-proxy),
# each declaring a different [agent] name; coral-server resolves which
# identity a launched container has from the manifest that named it, not
# from anything in the image itself. See src/main.rs for why this exists.
#
# Build from the repo root so the workspace (Cargo.toml/Cargo.lock + every
# member Cargo.toml, needed for workspace resolution even though only
# idle-agent's own dependency graph is actually compiled) is in the build
# context:
#   docker build -f crates/agents/idle-agent/Dockerfile -t idle-agent:0.1.0 .

FROM rust:1-slim-bookworm AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY native ./native

RUN cargo build --release -p idle-agent

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/idle-agent /usr/local/bin/idle-agent

ENTRYPOINT ["/usr/local/bin/idle-agent"]
