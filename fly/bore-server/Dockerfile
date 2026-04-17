FROM rust:1.78-slim AS builder

RUN cargo install bore-cli

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/cargo/bin/bore /usr/local/bin/bore

EXPOSE 7835

# BORE_SECRET is injected at runtime via fly secrets
CMD ["sh", "-c", "bore server --secret \"${BORE_SECRET}\""]
