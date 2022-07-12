.PHONY: svc deps udeps prepare check nats run

svc:
	docker-compose -p presence -f docker/development/docker-compose.yml up

deps:
	rustup toolchain install nightly # for cargo-udeps
	cargo install cargo-udeps cargo-sort cargo-outdated --locked
	cargo install sqlx-cli --no-default-features --features native-tls,postgres

udeps:
	cargo +nightly udeps

prepare:
	cargo sqlx prepare -- --tests

check:
	cargo check
	cargo clippy

nats:
	nats stream add classrooms-reliable --creds=nats.creds --subjects='classrooms.>' --storage=memory --replicas=1 --retention=limits --discard=old

run:
	cargo run --features dotenv,local_ip
