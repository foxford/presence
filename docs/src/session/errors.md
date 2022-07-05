# Errors

## Recoverable session errors

### `terminated`

Occurs when the server is rebooted (graceful shutdown)

| Attribute       | Type   | Description                 |
|-----------------|--------|-----------------------------|
| type            | string | "recoverable_session_error" |
| payload[type]   | string | "terminated"                |
| payload[title]  | string | "terminated"                |
| payload[status] | int    | 422                         |

```mermaid
sequenceDiagram
    actor Agent
    participant Presence1
    participant Presence2
    participant Kubernetes

    Agent ->> Presence1: connect
    activate Agent
    activate Presence1
    Presence1 ->> Agent: success
    Kubernetes ->> Presence1: SIGTERM
    Presence1 ->> Agent: recoverable session error(type=terminated)
    Agent ->> Presence2: connect
    activate Presence2
    Presence2 ->> Agent: success
    Agent ->> Presence1: close connection
    deactivate Presence1
    note over Presence1: wait for 10s
    Presence1 ->> Presence1: close all active connections
    deactivate Presence2
    deactivate Agent
```

## Unrecoverable session errors

### `replaced`

Occurs when the agent opens the second session

| Attribute       | Type   | Description                   |
|-----------------|--------|-------------------------------|
| type            | string | "unrecoverable_session_error" |
| payload[type]   | string | "replaced"                    |
| payload[title]  | string | "replaced"                    |
| payload[status] | int    | 422                           |

#### On the same replica

```mermaid
sequenceDiagram
    actor Agent1
    participant Presence
    participant DB
    actor Agent2

    Agent1 ->> Presence: connect
    activate Agent1
    activate Presence
    Presence ->> DB: create session in table agent_session for Agent1
    DB ->> Presence: Session ID
    Presence ->> Agent1: success
    Agent2 ->> Presence: connect
    activate Agent2
    Presence ->> Agent1: replaced
    Presence ->> Presence: close connection for Agent1
    deactivate Agent1
    Presence ->> Presence: Set previous session ID for Agent2
    Presence ->> Agent2: success
    deactivate Agent2
    deactivate Presence
```

#### On another replica

```mermaid
sequenceDiagram
    actor Agent1
    participant Presence1
    participant DB
    participant Presence2
    actor Agent2

    Agent1 ->> Presence1: connect
    activate Agent1
    activate Presence1
    Presence1 ->> DB: create session in table agent_session for Agent1
    DB ->> Presence1: Session ID
    Presence1 ->> Agent1: success
    Agent2 ->> Presence2: connect
    activate Agent2
    activate Presence2
    Presence2 ->> Presence1: close connection for Agent1
    Presence1 ->> Agent1: replaced
    deactivate Agent1
    Presence1 ->> DB: move Agent1 session to history
    DB ->> DB: create session in table agent_session_history
    DB ->> DB: delete session from table agent_session
    Presence1 ->> Presence2: success
    deactivate Presence1
    Presence2 ->> DB: create session in table agent_session for Agent2
    DB ->> Presence2: Session ID
    Presence2 ->> Agent2: success
    deactivate Agent2
    deactivate Presence2
```

### `auth_timed_out`

Occurs when the server didn't receive the [connect_request](/session/api.html#connect-request) from the client at a given period of time

| Attribute       | Type   | Description                   |
|-----------------|--------|-------------------------------|
| type            | string | "unrecoverable_session_error" |
| payload[type]   | string | "auth_timed_out"              |
| payload[title]  | string | "auth_timed_out"              |
| payload[status] | int    | 422                           |

```mermaid
sequenceDiagram
    actor Agent
    participant Presence

    Agent ->> Presence: ws connect
    activate Agent
    activate Presence
    Presence ->> Presence: wait for 10s
    Presence ->> Agent: unrecoverable session error(type=auth_timed_out)
    Presence ->> Agent: close connection
    deactivate Agent
    deactivate Presence
```

### `unauthenticated`

Occurs when the server didn't receive a valid token from the client

| Attribute       | Type   | Description                   |
|-----------------|--------|-------------------------------|
| type            | string | "unrecoverable_session_error" |
| payload[type]   | string | "unauthenticated"             |
| payload[title]  | string | "unauthenticated"             |
| payload[status] | int    | 401                           |

```mermaid
sequenceDiagram
    actor Agent
    participant Presence

    Agent ->> Presence: ws connect
    activate Agent
    activate Presence
    Agent ->> Presence: connect request w/ invalid token
    Presence->>Agent: unrecoverable session error(type=unauthenticated)

    deactivate Agent
    deactivate Presence
```

### `pong_timed_out`

Occurs when the server didn't receive the `PONG` message from the client at a given period of time

| Attribute       | Type   | Description                   |
|-----------------|--------|-------------------------------|
| type            | string | "unrecoverable_session_error" |
| payload[type]   | string | "pong_timed_out"              |
| payload[title]  | string | "pong_timed_out"              |
| payload[status] | int    | 422                           |

```mermaid
sequenceDiagram
    actor Agent
    participant Presence

    Agent ->> Presence: ws connect
    activate Agent
    activate Presence
    Presence ->> Agent: success

    Presence->>Agent: ping
    Presence->>Presence: wait for 5s
    Presence->>Agent: unrecoverable session error(type=pong_timed_out)
    Presence->>Agent: close connection

    deactivate Agent
    deactivate Presence
```
