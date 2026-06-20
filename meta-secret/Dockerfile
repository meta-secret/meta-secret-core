# ============================================================
# Multi-stage Dockerfile for meta-secret builds
# ============================================================

ARG RUST_VERSION=1.96.0
ARG PLAYWRIGHT_VERSION=1.61.0

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

RUN curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C /usr/local/bin

RUN curl https://wasm-bindgen.github.io/wasm-pack/installer/init.sh -sSf | sh

ENV BASE_DIR="/meta-secret-build"
WORKDIR ${BASE_DIR}

# Incremental compiles for dev/test profile; cache mounts below persist target/ across builds.
ENV CARGO_INCREMENTAL=1

# --- Builder Debug: pre-compiled deps in debug mode (for tests) ---
FROM builder-base AS builder-debug

COPY recipe.json .
RUN --mount=type=cache,target=/meta-secret-build/target,id=ms-target-debug \
    --mount=type=cache,target=/usr/local/cargo/registry,id=ms-cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=ms-cargo-git \
    cargo chef cook --tests --recipe-path recipe.json

# --- Builder Release: pre-compiled deps in release mode (for server and wasm) ---
FROM builder-base AS builder-release

COPY recipe.json .
RUN --mount=type=cache,target=/meta-secret-build/target,id=ms-target-release \
    --mount=type=cache,target=/usr/local/cargo/registry,id=ms-cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=ms-cargo-git \
    cargo chef cook --release --recipe-path recipe.json

# --- Prepare DB: run diesel migrations ---
FROM builder-release AS prepare-db

ARG DIESEL_VERSION=2.3.9
RUN curl -LsSf "https://github.com/diesel-rs/diesel/releases/download/v${DIESEL_VERSION}/diesel_cli-x86_64-unknown-linux-gnu.tar.xz" | \
    tar xJ -C /usr/local/bin diesel

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
RUN --mount=type=cache,target=/meta-secret-build/target,id=ms-target-release \
    --mount=type=cache,target=/usr/local/cargo/registry,id=ms-cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=ms-cargo-git \
    cargo build --release

# --- Meta Server: final runtime image ---
FROM debian:bookworm AS meta-server

RUN apt update && apt install -y ca-certificates curl iputils-ping sqlite3

WORKDIR /meta-secret

COPY --from=meta-server-builder /meta-secret-build/target/release/meta-server .
COPY --from=prepare-db /meta-secret-build/db/sqlite/build/meta-secret.db ./meta-secret.db

EXPOSE 3000
CMD ["./meta-server"]

# --- Builder WASM: pre-compile deps for wasm32-unknown-unknown target ---
FROM builder-base AS builder-wasm

RUN rustup target add wasm32-unknown-unknown

# Shared with local wasm/.cargo/config.toml; workspace config ensures chef + cargo agree.
RUN mkdir -p .cargo && printf '%s\n' \
    '[target.wasm32-unknown-unknown]' \
    'rustflags = ["--cfg=getrandom_backend=\"wasm_js\""]' \
    > .cargo/config.toml

COPY recipe.json .
RUN --mount=type=cache,target=/meta-secret-build/target,id=ms-target-wasm32 \
    --mount=type=cache,target=/usr/local/cargo/registry,id=ms-cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=ms-cargo-git \
    cargo chef cook --release --target wasm32-unknown-unknown --recipe-path recipe.json -p meta-secret-wasm

# --- WASM Builder: build wasm package (deps already cached in builder-wasm) ---
FROM builder-wasm AS wasm-builder

ARG WASM_BINDGEN_VERSION=0.2.125
RUN curl -LsSf "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${WASM_BINDGEN_VERSION}/wasm-bindgen-${WASM_BINDGEN_VERSION}-x86_64-unknown-linux-musl.tar.gz" | \
    tar xz -C /usr/local/bin --strip-components=1 "wasm-bindgen-${WASM_BINDGEN_VERSION}-x86_64-unknown-linux-musl/wasm-bindgen"

WORKDIR /meta-secret-build
# Workspace members only — web-cli is not a Cargo crate; omitting it avoids invalidating
# Rust layers when UI sources change.
COPY Cargo.toml recipe.json ./
COPY core db meta-server wasm cli meta-cli tests mobile .config ./

# cargo from workspace root reuses chef-cached deps; wasm-pack cd's into wasm/ and rebuilds all deps.
RUN --mount=type=cache,target=/meta-secret-build/target,id=ms-target-wasm32 \
    --mount=type=cache,target=/usr/local/cargo/registry,id=ms-cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=ms-cargo-git \
    cargo build --release --target wasm32-unknown-unknown -p meta-secret-wasm \
    && mkdir -p wasm/pkg \
    && wasm-bindgen target/wasm32-unknown-unknown/release/meta_secret_wasm.wasm \
        --out-dir wasm/pkg --target web --out-name meta_secret_wasm \
    && cp wasm/pkg.package.json wasm/pkg/package.json

# Minimal stage to extract just the wasm pkg
FROM scratch AS wasm-output
COPY --from=wasm-builder /meta-secret-build/wasm/pkg/ /

# --- Test Compiler: compile test binaries only (no execution) ---
FROM builder-debug AS test-compiler

WORKDIR /meta-secret-build
COPY Cargo.toml recipe.json ./
COPY core db meta-server wasm cli meta-cli tests mobile .config ./
RUN --mount=type=cache,target=/meta-secret-build/target,id=ms-target-debug \
    --mount=type=cache,target=/usr/local/cargo/registry,id=ms-cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=ms-cargo-git \
    cargo nextest run --no-run --profile ci

# --- Test Runner: execute pre-compiled tests ---
FROM test-compiler AS test-runner
# RUST_MIN_STACK: split/redistribution tests have deep async stacks that overflow the 2MiB default
RUN --mount=type=cache,target=/meta-secret-build/target,id=ms-target-debug \
    --mount=type=cache,target=/usr/local/cargo/registry,id=ms-cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=ms-cargo-git \
    RUST_MIN_STACK=8388608 cargo nextest run --profile ci

# --- Web Build: Vue.js + WASM ---
FROM oven/bun:1.2-debian AS web-build

WORKDIR /meta-secret/web-cli/ui

COPY web-cli/ui/package.json web-cli/ui/bun.lock ./
COPY --from=wasm-builder /meta-secret-build/wasm/pkg ./pkg
RUN bun install --frozen-lockfile

COPY web-cli/ui/ .
RUN bun run lint:check && bun run build

# Final web image
FROM web-build AS web
EXPOSE 5173

# Minimal stage to extract just the dist output
FROM scratch AS web-output
COPY --from=web-build /meta-secret/web-cli/ui/dist/ /

# --- Playwright: smoke test runner (browsers pre-installed in base image) ---
FROM mcr.microsoft.com/playwright:v${PLAYWRIGHT_VERSION}-noble AS playwright

WORKDIR /app

COPY web-cli/ui/package.json ./
RUN npm install --legacy-peer-deps @playwright/test@${PLAYWRIGHT_VERSION}

COPY web-cli/ui/playwright.config.ts ./
COPY web-cli/ui/tests/e2e ./tests/e2e

ENTRYPOINT ["npx", "playwright", "test"]
