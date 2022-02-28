.PHONY: deps db udeps

db:
	docker-compose -p presence -f docker/development/docker-compose.yml up

deps:
	rustup toolchain install nightly # for cargo-udeps
	cargo install cargo-udeps cargo-sort cargo-outdated --locked
	cargo install sqlx-cli --no-default-features --features native-tls,postgres

udeps:
	cargo +nightly udeps

prepare:
	cargo sqlx prepare -- --tests
