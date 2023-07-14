FROM --platform=$BUILDPLATFORM rust:1.71.0 as builder

ARG PKG_CONFIG_ALLOW_CROSS=1

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Download dependencies before forking for each target architecture,
# so the download is only performed once.
# Only copying Cargo.toml and Cargo.lock allows for better caching.
RUN cargo new --bin /app
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN cargo fetch

ARG TARGETPLATFORM
RUN echo "Configuring environment for ${TARGETPLATFORM:=linux/amd64}" && \
    case "${TARGETPLATFORM}" in \
    linux/amd64) \
        echo "x86_64-unknown-linux-gnu" | tee /tmp/target;; \
    linux/arm64) \
        echo "aarch64-unknown-linux-gnu" | tee /tmp/target;; \
    linux/arm/v7) \
        echo "armv7-unknown-linux-gnueabihf" | tee /tmp/target;; \
    *) \
        echo "Invalid platform: ${TARGETPLATFORM}";; \
    esac && \
    rustup target add "$(cat /tmp/target)"

# hadolint ignore=DL3008
RUN dpkg --add-architecture arm64 &&\
    dpkg --add-architecture armhf && \
    apt-get update && \ 
    apt-get install -y --no-install-recommends \
    pkg-config libssl-dev cmake g++ \
    libssl-dev:arm64 gcc-aarch64-linux-gnu g++-aarch64-linux-gnu \
    libssl-dev:armhf gcc-arm-linux-gnueabihf g++-arm-linux-gnueabihf

COPY . .
RUN cargo build --release --target "$(cat /tmp/target)" && \
    mv ./target/"$(cat /tmp/target)"/release/gantry-crane ./target/release/


FROM debian:bullseye-slim

WORKDIR /app

COPY --from=builder /app/target/release/gantry-crane /app/gantry-crane

ENTRYPOINT [ "/app/gantry-crane" ]
