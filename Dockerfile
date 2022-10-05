FROM rust as builder

RUN apt-get update && apt-get install -y --no-install-recommends cmake

WORKDIR /build
COPY . /build

RUN cargo build --release


FROM debian:bullseye-slim

WORKDIR /app

COPY --from=builder /build/target/release/gantry-crane /app/gantry-crane

ENTRYPOINT [ "/app/gantry-crane" ]
