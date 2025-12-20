FROM rust:1.87-bookworm AS builder

WORKDIR /build

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Pin home crate to version compatible with Rust 1.87
RUN cargo update home@0.5.12 --precise 0.5.9

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    git \
    docker.io \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN curl -L https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64 \
    -o /usr/local/bin/cloudflared && chmod +x /usr/local/bin/cloudflared

COPY --from=builder /build/target/release/foundryd /usr/local/bin/
COPY --from=builder /build/target/release/foundry-agent /usr/local/bin/

COPY migrations /migrations

ENV RUST_LOG=info

CMD ["foundryd"]
