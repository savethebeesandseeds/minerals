# syntax=docker/dockerfile:1.7

ARG RUST_VERSION=1.96.0

FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock askama.toml ./
COPY crates ./crates
COPY src ./src
COPY static ./static

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --locked --release \
    && install -Dm0755 target/release/minerals /out/minerals \
    && strip /out/minerals

FROM debian:bookworm-slim AS runtime

LABEL org.opencontainers.image.title="Waajacu's Minerals" \
      org.opencontainers.image.description="Private mineral catalog administration service" \
      org.opencontainers.image.licenses="AGPL-3.0-only"

ENV PORT=7979 \
    RUST_LOG=minerals=info \
    HOME=/tmp \
    XDG_CACHE_HOME=/tmp

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        util-linux \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 minerals \
    && useradd --uid 10001 --gid minerals --no-create-home \
        --home-dir /tmp --shell /usr/sbin/nologin minerals \
    && install -d -m0755 -o minerals -g minerals /app \
    && install -d -m0700 -o minerals -g minerals /app/data /app/data/minerals

ENV BIND_ADDRESS=0.0.0.0 \
    DATA_ROOT=/app/data

WORKDIR /app

COPY --from=builder /out/minerals /usr/local/bin/minerals
COPY --chmod=0755 docker-entrypoint.sh /usr/local/bin/minerals-entrypoint

USER minerals:minerals

EXPOSE 7979

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD ["curl", "--fail", "--silent", "--show-error", "--max-time", "4", "http://127.0.0.1:7979/readyz"]

STOPSIGNAL SIGTERM

ENTRYPOINT ["/usr/local/bin/minerals-entrypoint"]
