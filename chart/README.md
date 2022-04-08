# Nats gatekeeper helm chart

Helm chart for [presence](https://github.com/foxford/presence/)

## Prerequisites

`nats-credentials` secret must be present with nats key.

`.der` keys for each audience should exist to authenticate tokens
(see `.audiences.*.authn.key` and `.container.{volumes,volumeMounts}` in `values.yaml`)

## Installation

To install gatekeeper cd into this dir and run
```
helm install presence . --atomic -n testing01
```

## Tests

You can check that installation completed (somewhat) successfully with
```
helm test presence -n testing01
```

## Removal

To get rid of this chart run
```
helm uninstall presence -n helm-test-shkh
```
