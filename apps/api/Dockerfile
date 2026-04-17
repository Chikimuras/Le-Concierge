# syntax=docker/dockerfile:1.7
#
# Multi-stage build for the `api` binary.
#
# Build context MUST be the workspace root so the builder can access
# `Cargo.toml`, `Cargo.lock`, and every crate under `apps/` and `packages/`.
#
#   docker build -f apps/api/Dockerfile -t le-concierge/api:latest .
#
# Security notes (ADR 0002):
# - Runtime base is `gcr.io/distroless/cc-debian12:nonroot` — minimal attack
#   surface (no shell, no package manager), runs as uid 65532.
#   Ref: CIS Docker Benchmark 4.1, 4.6.
# - Binary is stripped and statically linked against rustls (no system
#   OpenSSL needed).

# ---------- stage 1: cargo-chef prepare (dependency graph only) -------------
FROM rust:1.85-slim-bookworm AS chef
WORKDIR /build
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config \
 && rm -rf /var/lib/apt/lists/*
RUN cargo install --locked cargo-chef

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---------- stage 2: cook dependencies, then build the binary ----------------
FROM chef AS builder
ENV CARGO_TERM_COLOR=never \
    SQLX_OFFLINE=true
COPY --from=planner /build/recipe.json recipe.json
# This layer is cached as long as `recipe.json` (== dependency graph) is stable.
RUN cargo chef cook --release --recipe-path recipe.json -p api

COPY . .
RUN cargo build --release --locked -p api \
 && strip target/release/api

# ---------- stage 3: minimal runtime ----------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app

# Config defaults baked in so the image boots without a bind-mount in
# non-prod environments. Real deploys overlay APP_CONFIG_FILE or env vars.
COPY --from=builder /build/apps/api/config /app/config
COPY --from=builder /build/target/release/api /app/api

# `distroless:nonroot` sets USER 65532 implicitly, but restate it for clarity.
USER 65532:65532
EXPOSE 3000

# No HEALTHCHECK here — orchestrators should probe GET /healthz directly.
ENTRYPOINT ["/app/api"]
