# syntax=docker/dockerfile:1

# ---------- 构建阶段 ----------
FROM rust:1-bookworm AS builder
WORKDIR /app

# 先只拷 Cargo.toml 并编译一个空壳，充分利用 Docker 层缓存
COPY Cargo.toml ./
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs \
    && cargo build --release \
    && rm -rf src

COPY src ./src
RUN touch src/main.rs && cargo build --release

# ---------- 运行阶段 ----------
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /data --shell /usr/sbin/nologin tomatossh \
    && mkdir -p /data \
    && chown -R 10001:10001 /data

COPY --from=builder /app/target/release/tomatossh-sync /usr/local/bin/tomatossh-sync

ENV BIND_ADDR=0.0.0.0:8787 \
    DATABASE_PATH=/data/tomatossh.db \
    RUST_LOG=info

VOLUME ["/data"]
EXPOSE 8787
USER 10001
WORKDIR /data

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8787/healthz || exit 1

ENTRYPOINT ["/usr/local/bin/tomatossh-sync"]
