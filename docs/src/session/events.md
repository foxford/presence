# Events

### `agent.entered`

Arrives when someone enters the classroom

Subject: `classroom.{:CLASSROOM_ID}.agent`

| Attribute | Type   | Description          |
|-----------|--------|----------------------|
| id        | object | Event ID             |
| payload   | object | Payload of the event |


#### Event ID

| Attribute   | Type   | Description |
|-------------|--------|-------------|
| entity_type | string | "agent"     |
| operation   | string | "entered"   |
| sequence_id | int    | Session ID  |

#### Payload

| Attribute   | Type   | Description |
|-------------|--------|-------------|
| version     | string | "v1"        |
| entity_type | string | "agent"     |
| label       | string | "entered"   |
| agent_id    | string | Agent ID    |

#### Example

```json
{
    "id": {
        "entity_type": "agent",
        "operation": "entered",
        "sequence_id": 1
    },
    "payload":{
        "version": "v1",
        "entity_type": "agent",
        "label": "entered",
        "agent_id": "dev.testing01.svc.foxford.ru"
    }
}
```

```mermaid
sequenceDiagram
    actor Agent
    participant Presence
    participant DB
    participant Nats

    Agent ->> Presence: ws connect
    activate Agent
    activate Presence
    Agent ->> Presence: connect request w/ token
    Presence ->> DB: create session in table agent_session for Agent
    Presence ->> Nats: send event "agent.entered"
    activate Nats
    Presence ->> Nats: subscribe to subject "classroom.<classroom_id>.*"
    Presence ->> Agent: success

    loop Every 30s
        Presence->>Agent: ping
        Agent->>Presence: pong
    end

    loop Constantly
      Nats ->> Nats: get new events from other services
      Nats ->> Presence: send new events for Agent
      Presence ->> Agent: resend new events for Agent
    end

    deactivate Agent
    deactivate Presence
    deactivate Nats
```

### `agent.left`

Arrives when someone leaves the classroom

Subject: `classroom.{:CLASSROOM_ID}.agent`

| Attribute | Type   | Description          |
|-----------|--------|----------------------|
| id        | object | Event ID             |
| payload   | object | Payload of the event |


#### Event ID

| Attribute   | Type   | Description |
|-------------|--------|-------------|
| entity_type | string | "agent"     |
| operation   | string | "left"      |
| sequence_id | int    | Session ID  |

#### Payload

| Attribute   | Type   | Description |
|-------------|--------|-------------|
| version     | string | "v1"        |
| entity_type | string | "agent"     |
| label       | string | "left"      |
| agent_id    | string | Agent ID    |

#### Example

```json
{
    "id": {
        "entity_type": "agent",
        "operation": "left",
        "sequence_id": 1
    },
    "payload":{
        "version": "v1",
        "entity_type": "agent",
        "label": "left",
        "agent_id": "dev.testing01.svc.foxford.ru"
    }
}
```

```mermaid
sequenceDiagram
    actor Agent
    participant Presence
    participant Nats
    participant DB

    Agent ->> Presence: close connection
    activate Presence
    Presence ->> Nats: send event "agent.left"
    Presence ->> DB: move Agent session to history
    DB ->> DB: create session in table agent_session_history
    DB ->> DB: delete session from table agent_session
    deactivate Presence
```
