# Stage 1: Build
FROM rust:1.94-bookworm AS builder
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY config/ config/
RUN cargo build --release --bin uc-server

# Stage 2: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/uc-server /usr/local/bin/uc-server
COPY config/default.toml /etc/uc-server/config.toml

ENV UC_SERVER_LISTEN=0.0.0.0:8080
ENV UC_SERVER_DATA_DIR=/var/lib/uc-server
ENV RUST_LOG=info

EXPOSE 8080
VOLUME ["/var/lib/uc-server"]

ENTRYPOINT ["uc-server"]
CMD ["--config", "/etc/uc-server/config.toml"]
