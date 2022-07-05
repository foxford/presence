# Events

### `agent.enter`

Arrives when someone enters the classroom

Subject: `classrooms.{:CLASSROOM_ID}.presence`

Attribute | Type   | Optional  | Description
----------| ------ |-----------| -----------
type      | string |           | "event"
label     | string |           | "agent.enter"
payload   | string | +         | Agent ID

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
    Presence ->> Nats: subscribe to subject "classrooms.<classroom_id>.*"
    activate Nats
    Presence ->> Nats: send event "agent.enter"
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

### `agent.leave`

Arrives when someone leaves the classroom

Subject: `classrooms.{:CLASSROOM_ID}.presence`

Attribute | Type   | Optional  | Description
----------| ------ |-----------| -----------
type      | string |           | "event"
label     | string |           | "agent.leave"
payload   | string | +         | Agent ID

```mermaid
sequenceDiagram
    actor Agent
    participant Presence
    participant Nats
    participant DB

    Agent ->> Presence: close connection
    activate Presence
    Presence ->> Nats: send event "agent.leave"
    Presence ->> DB: move Agent session to history
    DB ->> DB: create session in table agent_session_history
    DB ->> DB: delete session from table agent_session
    deactivate Presence
```
