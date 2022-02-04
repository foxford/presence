.PHONY: deps db

deps:
	cargo install sqlx-cli --no-default-features --features native-tls,postgres

db:
	docker-compose -p presence -f docker/development/docker-compose.yml up
