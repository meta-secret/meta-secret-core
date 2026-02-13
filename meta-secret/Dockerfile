# ============================================================
# Multi-stage Dockerfile for meta-secret builds
# ============================================================

ARG RUST_VERSION=1.93.0

# --- Chef Planner: generate cargo-chef recipe ---
FROM lukemathwalker/cargo-chef:latest-rust-${RUST_VERSION}-bookworm AS chef-planner
COPY . .
RUN rm -f .cargo/config.toml
RUN cargo chef prepare --recipe-path recipe.json

# Minimal stage to extract just the recipe file
FROM scratch AS recipe-output
COPY --from=chef-planner /recipe.json /recipe.json

# --- Builder: base image with all Rust tooling + cached deps ---
FROM rust:${RUST_VERSION}-bookworm AS builder

RUN rustup component add rustfmt

# Install sccache (architecture-aware)
ENV SCCACHE_VERSION="v0.10.0"
ENV RUSTC_WRAPPER=sccache

RUN ARCH=$(uname -m) && \
    SCCACHE_ARTIFACT="sccache-${SCCACHE_VERSION}-${ARCH}-unknown-linux-musl" && \
    wget https://github.com/mozilla/sccache/releases/download/${SCCACHE_VERSION}/${SCCACHE_ARTIFACT}.tar.gz \
    && tar xzf ${SCCACHE_ARTIFACT}.tar.gz \
    && mv ${SCCACHE_ARTIFACT}/sccache /usr/local/bin/sccache \
    && chmod +x /usr/local/bin/sccache

RUN cargo install cargo-chef --locked
RUN cargo install wasm-pack --version 0.13.1 --locked
RUN cargo install diesel_cli --no-default-features --features sqlite

ENV BASE_DIR="/meta-secret-build"
WORKDIR ${BASE_DIR}

COPY --from=chef-planner /recipe.json .
RUN cargo chef cook --release --recipe-path recipe.json

# --- Prepare DB: run diesel migrations ---
FROM builder AS prepare-db

WORKDIR /meta-secret-build
RUN mkdir -p db
COPY db ./db

ENV SQLITE_DIR=/meta-secret-build/db/sqlite
RUN mkdir -p ${SQLITE_DIR}/build
WORKDIR ${SQLITE_DIR}

ENV DATABASE_URL=build/meta-secret.db
RUN diesel migration run

# --- Meta Server Builder: compile meta-server binary ---
FROM builder AS meta-server-builder

WORKDIR /meta-secret-build
COPY . .
RUN rm -f .cargo/config.toml

WORKDIR /meta-secret-build/meta-server/web-server
RUN cargo build --release

# --- Meta Server: final runtime image ---
FROM debian:bookworm AS meta-server

RUN apt update && apt install -y ca-certificates curl iputils-ping sqlite3

WORKDIR /meta-secret

COPY --from=meta-server-builder /meta-secret-build/target/release/meta-server .
COPY --from=prepare-db /meta-secret-build/db/sqlite/build/meta-secret.db ./meta-secret.db

EXPOSE 3000
CMD ["./meta-server"]

# --- WASM Builder: build wasm package ---
FROM builder AS wasm-builder

RUN rustup target add wasm32-unknown-unknown

WORKDIR /meta-secret-build
COPY . .
RUN rm -f .cargo/config.toml

WORKDIR /meta-secret-build/wasm
RUN wasm-pack build --target web

# Minimal stage to extract just the wasm pkg
FROM scratch AS wasm-output
COPY --from=wasm-builder /meta-secret-build/wasm/pkg/ /

# --- Test Runner: run tests with cargo-nextest ---
FROM builder AS test-runner

WORKDIR /meta-secret-build

RUN cargo install --locked cargo-nextest

COPY . .
RUN rm -f .cargo/config.toml

RUN cargo nextest run --profile ci

# --- Web Build: Vue.js + WASM ---
FROM node:23.9.0-bookworm AS web-build

WORKDIR /meta-secret/web-cli/ui

# Cache npm dependencies
COPY web-cli/ui/package.json .
RUN npm install && npm install --dev

COPY --from=wasm-builder /meta-secret-build/wasm/pkg ./pkg

WORKDIR /meta-secret/web-cli
COPY web-cli/ .

WORKDIR /meta-secret/web-cli/ui
RUN rm -f package-lock.json && npm install && npm run build

# Final web image
FROM web-build AS web
EXPOSE 5173

# Minimal stage to extract just the dist output
FROM scratch AS web-output
COPY --from=web-build /meta-secret/web-cli/ui/dist/ /
