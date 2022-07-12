FROM rust:1.60.0-slim-buster

RUN apt update && apt install -y --no-install-recommends \
  pkg-config \
  libssl-dev \
  libcurl4-openssl-dev \
  libpq-dev

RUN cargo install sqlx-cli --no-default-features --features native-tls,postgres
WORKDIR /app
COPY ./migrations /app/migrations
COPY Cargo.toml /app/Cargo.toml
COPY Cargo.lock /app/Cargo.lock
CMD ["cargo", "sqlx", "migrate", "run"]
