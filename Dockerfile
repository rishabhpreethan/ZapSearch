# Multi-stage build for Rust server
FROM rust:1.78-slim AS builder
WORKDIR /app
# Cache deps
COPY Cargo.toml ./
COPY core/Cargo.toml core/Cargo.toml
COPY indexer/Cargo.toml indexer/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml
RUN mkdir -p core/src indexer/src server/src \
    && echo "pub fn main(){}" > server/src/main.rs \
    && echo "" > core/src/lib.rs \
    && echo "fn main(){}" > indexer/src/main.rs \
    && cargo build -p server --release || true
# Build with sources
COPY . .
RUN cargo build -p server --release

FROM debian:bookworm-slim
RUN useradd -m app && mkdir -p /data/index && chown -R app:app /data
USER app
COPY --from=builder /app/target/release/server /usr/local/bin/server
WORKDIR /data
ENV PORT=8080
EXPOSE 8080
CMD ["/usr/local/bin/server", "--index", "/data/index", "--host", "0.0.0.0", "--port", "8080"]
