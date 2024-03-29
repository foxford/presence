## -----------------------------------------------------------------------------
## Build
## -----------------------------------------------------------------------------
FROM rust:1.70.0-slim-buster as build-stage

RUN apt update && apt install -y --no-install-recommends \
  pkg-config \
  libssl-dev \
  libcurl4-openssl-dev \
  libpq-dev

WORKDIR "/build"

# Install and build crates
COPY Cargo.* /build/
RUN mkdir /build/src && echo "fn main() {}" > /build/src/main.rs
RUN cargo build --release

# Build app
COPY sqlx-data.json /build/sqlx-data.json
COPY src/ /build/src/
RUN touch src/main.rs && cargo build --release

## -----------------------------------------------------------------------------
## Package
## -----------------------------------------------------------------------------
FROM debian:buster

# Install runtime dependencies
RUN apt update && apt install -y --no-install-recommends \
  ca-certificates \
  libssl1.1 \
  libcurl4 \
  libpq5

COPY --from=build-stage "/build/target/release/presence" "/app/presence"

WORKDIR "/app"
ENTRYPOINT ["/app/presence"]
