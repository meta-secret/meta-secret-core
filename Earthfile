VERSION 0.8
FROM scratch
ENV RUST_VERSION="1.82.0"

generate-cargo-chef-recipe:
    FROM lukemathwalker/cargo-chef:latest-rust-${RUST_VERSION}-bookworm
    COPY . .
    RUN cargo chef prepare --recipe-path recipe.json
    SAVE ARTIFACT recipe.json AS LOCAL recipe.json

base-build:
    FROM rust:${RUST_VERSION}-bookworm

    RUN rustup component add rustfmt

    # Install sccache (cargo is too slow)
    ENV SCCACHE_VERSION="v0.9.0"
    #RUN cargo install sccache@${SCCACHE_VERSION}
    ENV RUSTC_WRAPPER=sccache

    RUN wget https://github.com/mozilla/sccache/releases/download/${SCCACHE_VERSION}/sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl.tar.gz \
        && tar xzf sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl.tar.gz \
        && mv sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl/sccache /usr/local/bin/sccache \
        && chmod +x /usr/local/bin/sccache

    # Install cargo-chef
    RUN cargo install cargo-chef --locked

    # Cache dependencies with cargo chef
    COPY recipe.json .
    RUN cargo chef cook --release --recipe-path recipe.json

wasm-build:
    FROM +base-build

    #RUN cargo install wasm-pack slooooow
    RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

    RUN cd wasm && wasm-pack build --target web

    COPY . .

    WORKDIR /wasm
    RUN wasm-pack build --target web
    SAVE ARTIFACT pkg

wasm-build-local:
    FROM +wasm-build
    SAVE ARTIFACT pkg AS LOCAL web-cli/ui/pkg

web-build:
    FROM node:22.7-bookworm

    ENV PROJECT_UI_DIR=web-cli/ui

    # Cache npm dependencies
    WORKDIR ${PROJECT_UI_DIR}
    COPY ${PROJECT_UI_DIR}/package.json .
    RUN npm install && npm install --dev

    COPY +wasm-build/pkg ./pkg

    WORKDIR /
    COPY . .

    WORKDIR ${PROJECT_UI_DIR}
    RUN npm install && npm run build
    SAVE IMAGE meta-secret-web:latest

app-test:
    BUILD +base-build
    FROM +base-build
    COPY . .

    RUN --no-cache cargo test --release
