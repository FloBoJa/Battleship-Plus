FROM rust as builder

WORKDIR /battleship_plus_build

RUN --mount=type=cache,target=/var/cache/apt \
    apt-get update \
    && DEBIAN_FRONTEND=noninteractive \
        apt-get install --no-install-recommends --assume-yes \
          protobuf-compiler

COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/battleship_plus_build/target \
    cargo install --target-dir ./target --path ./crates/battleship_plus_server battleship_plus_server --features silent

FROM debian:bullseye-slim

WORKDIR /battleship_plus

COPY --from=builder /usr/local/cargo/bin/battleship_plus_server server
RUN chmod +x /battleship_plus/server

EXPOSE 30305/udp
CMD ./server
