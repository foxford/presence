# Presence service

## Set up

Install dev-dependencies (`sqlx`):
```shell
$ make deps
```

Run db via docker-compose:
```shell
$ make db
```

Create database and run migrations:

```shell
$ sqlx database create
$ sqlx migrate run
```
