# API

### Routes
| Route | Method | Short description                   |
|-------|--------|-------------------------------------|
| /ws   | GET    | Establishes a WebSocket connection. |

### Connect request

After establishing the WebSocket connection, the user needs to send `connect_request` within 10 seconds.
Otherwise, the connection will close through the timeout.

Request parameters:

| Attribute | Type   | Description                 |
|-----------|--------|-----------------------------|
| type      | string | "connect_request".          |
| payload   | object | The payload of the request. |

Payload parameters:

| Attribute    | Type   | Description          |
|--------------|--------|----------------------|
| agent_label  | string | Agent label.         |
| classroom_id | string | Classroom ID (uuid). |
| token        | string | JWT token.           |

#### Successful response

```json
{ "type": "connect_success" }
```

#### Unsuccessful responses

* Unsupported request

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "unsupported_request", "title": "Unsupported request", "status": 405 }}
```

* [Unauthenticated](./errors.html#unauthenticated)

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "unauthenticated", "title": "Unauthenticated", "status": 401 }}
```

* Access denied

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "access_denied", "title": "Access denied", "status": 403 }}
```

* Internal server error

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "internal_server_error", "title": "Internal server error", "status": 500 }}
```

* Serialization failed

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "serialization_failed", "title": "Serialization failed", "status": 422 }}
```

* [AuthTimedOut](./errors.html#auth_timed_out)

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "auth_timed_out", "title": "Auth timed out", "status": 422 }}
```

* [PongTimedOut](./errors.html#pong_timed_out)

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "pong_timed_out", "title": "Pong timed out", "status": 422 }}
```

* [Replaced](./errors.html#replaced)

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "replaced", "title": "Replaced", "status": 422 }}
```
