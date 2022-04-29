# Presence service

## Set up

Install dev-dependencies (`sqlx`):

```shell
$ make deps
```

Run related services via docker-compose:

```shell
$ make svc
```

Create database and run migrations:

```shell
$ sqlx database create
$ sqlx migrate run
```

## Nats

Install Nats client:

```shell
$ brew tap nats-io/nats-tools
$ brew install nats-io/nats-tools/nats
```

Create a stream on Nats:

```shell
$ nats stream add classrooms-reliable --creds=nats.creds --subjects='classrooms.>' --storage=memory --replicas=1 --retention=limits --discard=old
# Next "Enter" for all questions 
```
