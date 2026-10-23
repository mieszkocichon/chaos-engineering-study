# Runtime image for chaos-svc / chaos-coord / chaos-gen (replaces when-gossip-goes-silent/Dockerfile).
FROM rust:1.97.1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p chaos --bins && \
    mkdir /out && cp target/release/chaos-svc target/release/chaos-coord target/release/chaos-gen target/release/chaos-run /out/

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      iproute2 iptables curl procps ca-certificates && \
    curl -L -o /tmp/k6.tar.gz \
      https://github.com/grafana/k6/releases/download/v0.55.0/k6-v0.55.0-linux-amd64.tar.gz && \
    tar xzf /tmp/k6.tar.gz -C /tmp && \
    mv /tmp/k6-v0.55.0-linux-amd64/k6 /usr/local/bin/k6 && \
    rm -rf /tmp/k6.tar.gz /tmp/k6-v0.55.0-linux-amd64 /var/lib/apt/lists/*
COPY --from=build /out/ /usr/local/bin/
WORKDIR /app
COPY when-gossip-goes-silent/runner/k6-script.js /app/runner/k6-script.js
COPY when-gossip-goes-silent/config /app/config
CMD ["chaos-svc"]
