# Presence service

## Set up

Install dev-dependencies (`sqlx`):

```shell
$ make deps
```

Run related services via docker-compose (postgres, nats):

```shell
$ make svc
```

Create database and run migrations:

```shell
$ sqlx database create
$ sqlx migrate run
```

## Nats

1. Install Nats client:

```shell
$ brew tap nats-io/nats-tools
$ brew install nats-io/nats-tools/nats
```

2. Place the `nats.creds` file in the root of the project.

3. Copy the `nats.conf` file and edit it:

```shell
cp docker/development/nats/nats.conf{.example,}
```

4. Create a stream on Nats:

```shell
$ nats stream add classroom-out --creds=nats.creds --subjects='classroom.>' --storage=memory --replicas=1 --retention=interest --discard=new
# Next "Enter" for all questions 
```

## Run the project

```shell
$ make run
```
