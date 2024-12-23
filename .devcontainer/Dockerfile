FROM debian:bookworm

RUN apt update && apt install -y \
    build-essential pkg-config libssl-dev \
    curl wget git bash-completion

#################### Installation ####################
#Earthly
ENV EARTHLY_VERSION="v0.8.15"
ENV EARTHLY_RELEASE="https://github.com/earthly/earthly/releases/download/${EARTHLY_VERSION}/earthly-linux-amd64"
RUN wget ${EARTHLY_RELEASE} -O /usr/local/bin/earthly \
      && chmod +x /usr/local/bin/earthly \
      && /usr/local/bin/earthly bootstrap --with-autocomplete

#Rust
ENV PATH="/root/.cargo/bin:${PATH}"
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
RUN rustup target add x86_64-unknown-linux-gnu

# Sccache (cargo is too slow)
#RUN cargo install sccache@0.8.1
ENV RUSTC_WRAPPER=sccache
SCCACHE_VERSION="v0.9.0"
RUN wget https://github.com/mozilla/sccache/releases/download/${SCCACHE_VERSION}/sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl.tar.gz \
        && tar xzf sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl.tar.gz \
        && mv sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl/sccache /usr/local/bin/sccache \
        && chmod +x /usr/local/bin/sccache

# Node.js and npm (with nvm)
RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.0/install.sh | bash
ENV NVM_DIR="/root/.nvm"
ENV NODE_VERSION="22.8.0"
RUN . $NVM_DIR/nvm.sh \
    && nvm install $NODE_VERSION \
    && nvm alias default $NODE_VERSION \
    && nvm use default

# Install cargo-chef
RUN cargo install cargo-chef --locked

################ Setup ################
#ENV META_PROJECT_DIR=/meta-secret
#WORKDIR ${META_PROJECT_DIR}
# Cache dependencies with cargo chef
#COPY recipe.json ${META_PROJECT_DIR}
#RUN cargo chef cook --release --target x86_64-unknown-linux-gnu --recipe-path recipe.json

#RUN cd wasm && wasm-pack build --target web
