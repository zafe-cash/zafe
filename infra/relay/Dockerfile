ARG RUST_VERSION=1.97

FROM rust:${RUST_VERSION}-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p zafe-relay \
    && install -Dm755 target/release/zafe-relay /out/zafe-relay

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends sqlite3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 zafe \
    && useradd --system --uid 10001 --gid zafe --home-dir /data --shell /usr/sbin/nologin zafe \
    && mkdir -p /data && chown zafe:zafe /data
COPY --from=build /out/zafe-relay /usr/local/bin/zafe-relay
COPY infra/relay/entrypoint.sh /usr/local/bin/entrypoint.sh

ENV PORT=8080 \
    ZAFE_RELAY_DB=/data/relay.sqlite \
    RUST_LOG=zafe_relay=info
VOLUME ["/data"]
EXPOSE 8080
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
