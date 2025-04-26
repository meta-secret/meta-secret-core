# https://docs.docker.com/develop/develop-images/multistage-build/
FROM rust:1.86.0-bookworm AS build
COPY ./ /build
WORKDIR /build
RUN cargo test
RUN cargo build --release

FROM ubuntu:24.04

WORKDIR /meta-secret

COPY --from=build /build/target/release/meta-cli /meta-secret/meta-cli
COPY config.yaml /meta-secret/config.yaml

ENTRYPOINT ["./meta-cli"]
