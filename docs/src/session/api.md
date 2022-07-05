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
{ "type": "unrecoverable_session_error", "payload": { "type": "unsupported_request", "title": "Unsupported request", "status": 422 }}
```

* [Unauthenticated](/session/errors.html#unauthenticated)

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "unauthenticated", "title": "Unauthenticated", "status": 401 }}
```

* Access denied

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "access_denied", "title": "Access denied", "status": 403 }}
```

* Database connection acquisition failed

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "database_connection_acquisition_failed", "title": "Database connection acquisition failed", "status": 422 }}
```

* Database query failed

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "database_query_failed", "title": "Database query failed", "status": 422 }}
```

* Serialization failed

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "serialization_failed", "title": "Serialization failed", "status": 422 }}
```

* Messaging failed

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "messaging_failed", "title": "Messaging failed", "status": 422 }}
```

* Close old connection failed ([on another replica](/session/errors.html#on-another-replica))

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "close_old_connection_failed", "title": "Close old connection failed", "status": 422 }}
```

* [AuthTimedOut](/session/errors.html#auth_timed_out)

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "auth_timed_out", "title": "Auth timed out", "status": 422 }}
```

* [PongTimedOut](/session/errors.html#pong_timed_out)

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "pong_timed_out", "title": "Pong timed out", "status": 422 }}
```

* [Replaced](/session/errors.html#replaced)

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "replaced", "title": "Replaced", "status": 422 }}
```
