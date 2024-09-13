FROM ubuntu:24.04

RUN apt update && apt install -y curl wget git build-essential

#################### Installation ####################
# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
#RUN rustup component add rustfmt

# Install Node.js and npm
# installs nvm (Node Version Manager)
RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.0/install.sh | bash
ENV NVM_DIR="~/.nvm"
ENV NODE_VERSION 22.8.0

RUN ls -la /root/.nvm && ls -la /root/.nvm/nvm.sh

RUN . $NVM_DIR/nvm.sh \
    && nvm install $NODE_VERSION \
    && nvm alias default $NODE_VERSION \
    && nvm use default
ENV PATH $NVM_DIR/versions/node/v$NODE_VERSION/bin:$PATH

################ Setup ################
# Install sccache (cargo is too slow)
#RUN cargo install sccache@0.8.1
ENV RUSTC_WRAPPER=sccache
RUN wget https://github.com/mozilla/sccache/releases/download/v0.8.1/sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz \
    && tar xzf sccache-v0.8.1-x86_64-unknown-linux-musl.tar.gz \
    && mv sccache-v0.8.1-x86_64-unknown-linux-musl/sccache /usr/local/bin/sccache \
    && chmod +x /usr/local/bin/sccache

# Install cargo-chef
RUN cargo install cargo-chef --locked

# Cache dependencies with cargo chef
RUN cargo chef cook --release --recipe-path recipe.json

RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

RUN cd wasm && wasm-pack build --target web

RUN cd web-cli/ui npm install
