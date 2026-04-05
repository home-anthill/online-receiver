# syntax=docker/dockerfile:1

# ── Stage 1: Chef setup ──────────────────────────────────────────────────────
FROM rust:trixie AS chef

# some cargo dependencies require additional packages to build the project.
RUN apt-get update && apt-get install -y \
    g++ \
    openssl \
    make cmake

WORKDIR /app

RUN cargo install cargo-chef


# ── Stage 2: Planner ─────────────────────────────────────────────────────────
FROM chef AS planner

COPY . .

RUN cargo chef prepare --recipe-path recipe.json


# ── Stage 3: Builder ─────────────────────────────────────────────────────────
FROM chef AS builder

WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json

# copy source code and build it
COPY . .

RUN cargo build --release


# ── Stage 4: Collect CA certs and runtime shared libraries ───────────────────
# The hardened runtime image has no package manager, so we install here and
# copy what we need into the final stage.
FROM debian:trixie-slim AS system-deps

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates

# Pre-create the app directory owned by nobody (uid/gid 65534) so the final
# stage never needs to run a RUN command as root.
RUN mkdir -p /app/logs && chown -R 65534:65534 /app


# ── Stage 5: Hardened runtime ────────────────────────────────────────────────
# dhi.io/debian-base:trixie — no package manager, no root user, shell present.
FROM dhi.io/debian-base:trixie AS runtime

# CA certificates for TLS (includes ISRG_Root_X1.pem used in prod).
COPY --from=system-deps /etc/ssl/certs /etc/ssl/certs

# App directory skeleton (/app and /app/logs owned by nobody).
COPY --from=system-deps --chown=65534:65534 /app /app

WORKDIR /app

# Binary and env template.
COPY --from=builder --chown=65534:65534 /app/target/release/online /app/online

USER 65534

ENTRYPOINT ["/app/online"]
