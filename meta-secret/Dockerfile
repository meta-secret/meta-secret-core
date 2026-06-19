# ============================================================
# Multi-stage Dockerfile for meta-secret builds
# ============================================================

ARG RUST_VERSION=1.96.0

# --- Chef Planner: generate cargo-chef recipe ---
FROM lukemathwalker/cargo-chef:latest-rust-${RUST_VERSION}-bookworm AS chef-planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Minimal stage to extract just the recipe file
FROM scratch AS recipe-output
COPY --from=chef-planner /recipe.json /recipe.json

# --- Builder Base: Rust tooling only, no dep pre-compilation ---
FROM rust:${RUST_VERSION}-bookworm AS builder-base

RUN rustup component add rustfmt

COPY --from=chef-planner /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef

RUN ARCH=$(uname -m) && \
    NEXTEST_ARCH=$([ "$ARCH" = "aarch64" ] && echo "linux-arm" || echo "linux") && \
    curl -LsSf https://get.nexte.st/latest/${NEXTEST_ARCH} | tar zxf - -C /usr/local/bin

RUN curl https://wasm-bindgen.github.io/wasm-pack/installer/init.sh -sSf | sh

ENV BASE_DIR="/meta-secret-build"
WORKDIR ${BASE_DIR}

# --- Builder Debug: pre-compiled deps in debug mode (for tests) ---
FROM builder-base AS builder-debug

COPY recipe.json .
RUN cargo chef cook --jobs $(nproc) --recipe-path recipe.json

# --- Builder Release: pre-compiled deps in release mode (for server and wasm) ---
FROM builder-base AS builder-release

COPY recipe.json .
RUN cargo chef cook --jobs $(nproc) --release --recipe-path recipe.json

# --- Prepare DB: run diesel migrations ---
FROM builder-release AS prepare-db

RUN cargo install diesel_cli --no-default-features --features sqlite

WORKDIR /meta-secret-build
RUN mkdir -p db
COPY db ./db

ENV SQLITE_DIR=/meta-secret-build/db/sqlite
RUN mkdir -p ${SQLITE_DIR}/build
WORKDIR ${SQLITE_DIR}

ENV DATABASE_URL=build/meta-secret.db
RUN diesel migration run

# --- Meta Server Builder: compile meta-server binary ---
FROM builder-release AS meta-server-builder

WORKDIR /meta-secret-build
COPY . .

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
FROM builder-release AS wasm-builder

RUN rustup target add wasm32-unknown-unknown

WORKDIR /meta-secret-build
COPY . .

WORKDIR /meta-secret-build/wasm
RUN wasm-pack build --target web

# Minimal stage to extract just the wasm pkg
FROM scratch AS wasm-output
COPY --from=wasm-builder /meta-secret-build/wasm/pkg/ /

# --- Test Compiler: compile test binaries only (no execution) ---
FROM builder-debug AS test-compiler

WORKDIR /meta-secret-build
COPY . .
RUN cargo nextest run --no-run --profile ci

# --- Test Runner: execute pre-compiled tests ---
FROM test-compiler AS test-runner
# RUST_MIN_STACK: split/redistribution tests have deep async stacks that overflow the 2MiB default
RUN RUST_MIN_STACK=8388608 cargo nextest run --profile ci

# --- Web Build: Vue.js + WASM ---
FROM node:23.9.0-bookworm AS web-build

WORKDIR /meta-secret/web-cli/ui

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
