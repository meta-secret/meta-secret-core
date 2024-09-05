VERSION 0.8
FROM scratch

generate-cargo-chef-recipe:
    FROM lukemathwalker/cargo-chef:latest-rust-1.80-bookworm
    COPY . .
    RUN cargo chef prepare --recipe-path recipe.json
    SAVE ARTIFACT recipe.json AS LOCAL recipe.json

base-build:
    FROM rust:1.80.1

    # Install sccache (cargo is too slow)
    #RUN cargo install sccache@0.8.1
    ENV RUSTC_WRAPPER=sccache
    RUN wget https://github.com/mozilla/sccache/releases/download/v0.8.1/sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz \
        && tar xzf sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz \
        && mv sccache-v0.8.1-x86_64-unknown-linux-musl/sccache /usr/local/bin/sccache \
        && chmod +x /usr/local/bin/sccache

    # Install cargo-chef
    RUN cargo install cargo-chef --locked

    #RUN cargo install wasm-pack slooooow
    RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
    RUN rustup component add rustfmt

    # Cache dependencies with cargo chef
    COPY recipe.json .
    RUN cargo chef cook --release --recipe-path recipe.json
    RUN cd wasm && wasm-pack build

wasm-build:
    BUILD +base-build
    FROM +base-build
    COPY . .

    WORKDIR /wasm
    RUN wasm-pack build --target web
    SAVE ARTIFACT pkg

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

    RUN ls -la .

    WORKDIR ${PROJECT_UI_DIR}
    RUN npm run build
    SAVE IMAGE meta-secret-web:latest

app-test:
    BUILD +base-build
    FROM +base-build
    COPY . .

    RUN --no-cache cargo test --release