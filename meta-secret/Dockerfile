# ============================================================
# Multi-stage Dockerfile for meta-secret builds
# ============================================================

ARG RUST_VERSION=1.96.0

# --- Builder: Rust tooling + sccache for dep caching ---
FROM rust:${RUST_VERSION}-bookworm AS builder

RUN rustup component add rustfmt

# sccache caches compiled crates in /sccache-cache inside the image layer.
# Each RUN step starts a fresh daemon but reads the same on-disk cache,
# giving 100% cache hits on unchanged deps across stages.
ENV SCCACHE_VERSION="v0.15.0"
ENV SCCACHE_DIR="/sccache-cache"
ENV RUSTC_WRAPPER=sccache

RUN ARCH=$(uname -m) && \
    wget -q https://github.com/mozilla/sccache/releases/download/${SCCACHE_VERSION}/sccache-${SCCACHE_VERSION}-${ARCH}-unknown-linux-musl.tar.gz \
    && tar xzf sccache-${SCCACHE_VERSION}-${ARCH}-unknown-linux-musl.tar.gz \
    && mv sccache-${SCCACHE_VERSION}-${ARCH}-unknown-linux-musl/sccache /usr/local/bin/sccache \
    && chmod +x /usr/local/bin/sccache

RUN ARCH=$(uname -m) && \
    NEXTEST_ARCH=$([ "$ARCH" = "aarch64" ] && echo "linux-arm" || echo "linux") && \
    curl -LsSf https://get.nexte.st/latest/${NEXTEST_ARCH} | tar zxf - -C /usr/local/bin
RUN curl https://wasm-bindgen.github.io/wasm-pack/installer/init.sh -sSf | sh

ENV BASE_DIR="/meta-secret-build"
WORKDIR ${BASE_DIR}

# Pre-warm sccache: compile all deps with full source, then discard target/.
# sccache artifacts stay in /sccache-cache and are inherited by all derived stages.
# This layer is invalidated only when Cargo.lock changes (dep versions change).
COPY . .
RUN cargo build 2>/dev/null || true
RUN cargo build --release 2>/dev/null || true
RUN rm -rf target

# --- Prepare DB: run diesel migrations ---
FROM builder AS prepare-db

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
FROM builder AS meta-server-builder

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
FROM builder AS wasm-builder

RUN rustup target add wasm32-unknown-unknown

WORKDIR /meta-secret-build
COPY . .

WORKDIR /meta-secret-build/wasm
ENV RUSTC_WRAPPER=""
RUN wasm-pack build --target web

# Minimal stage to extract just the wasm pkg
FROM scratch AS wasm-output
COPY --from=wasm-builder /meta-secret-build/wasm/pkg/ /

# --- Test Runner: run tests with cargo-nextest ---
FROM builder AS test-runner

WORKDIR /meta-secret-build
COPY . .
RUN cargo nextest run --profile ci

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
