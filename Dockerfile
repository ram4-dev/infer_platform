# Build stage
FROM rust:1.82-slim-bookworm AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

RUN cargo build --release --bin api-gateway --bin node-agent

# api-gateway runtime
FROM debian:bookworm-slim AS api-gateway

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/api-gateway /usr/local/bin/api-gateway

EXPOSE 8080
CMD ["api-gateway"]

# node-agent runtime
FROM debian:bookworm-slim AS node-agent

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/node-agent /usr/local/bin/node-agent

EXPOSE 8181
CMD ["node-agent"]
