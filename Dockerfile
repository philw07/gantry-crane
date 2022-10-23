FROM rust as builder

RUN apt-get update && apt-get install -y --no-install-recommends cmake

WORKDIR /build
COPY . /build

# tmpfs mount needed due to QEMU bug: https://github.com/rust-lang/cargo/issues/8719
RUN --mount=type=tmpfs,target=/.cargo CARGO_HOME=/.cargo cargo build --release


FROM debian:bullseye-slim

WORKDIR /app

COPY --from=builder /build/target/release/gantry-crane /app/gantry-crane

ENTRYPOINT [ "/app/gantry-crane" ]
