FROM rust:1.76-slim AS builder
WORKDIR /usr/src/rust-core
COPY rust-core/Cargo.toml rust-core/Cargo.lock* ./
COPY rust-core/ ./
RUN apt-get update && apt-get install -y pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
RUN cargo build --release --manifest-path Cargo.toml

FROM debian:bookworm-slim
COPY --from=builder /usr/src/rust-core/target/release/rust-core /usr/local/bin/rust-core
RUN chmod +x /usr/local/bin/rust-core
ENTRYPOINT ["/usr/local/bin/rust-core"]
