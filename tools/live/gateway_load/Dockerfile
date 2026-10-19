# syntax=docker/dockerfile:1
# Gateway under test (the loopback dev host of the gateway core) and the load tools.
# Build from the repository root:
#   docker build -f tools/live/gateway_load/Dockerfile --target gateway -t moonglow-load-gateway .
#   docker build -f tools/live/gateway_load/Dockerfile --target tools   -t moonglow-load-tools .
FROM rust:1.98-bookworm AS build
WORKDIR /src
COPY . .
ARG RUSTFLAGS=""
ENV CARGO_INCREMENTAL=0 NWN_ROOT=/nonexistent RUSTFLAGS=${RUSTFLAGS}
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p mg-gateway-dev && \
    install -D target/release/mg-gateway-dev /out/mg-gateway-dev

FROM debian:bookworm-slim AS gateway
COPY --from=build /out/mg-gateway-dev /usr/local/bin/mg-gateway-dev
WORKDIR /data
ENTRYPOINT ["/usr/local/bin/mg-gateway-dev"]

FROM python:3.12-slim-bookworm AS tools
COPY tools/live/gateway_load /load
COPY tools/live/event-stream.lua /load/event-stream.lua
WORKDIR /load
