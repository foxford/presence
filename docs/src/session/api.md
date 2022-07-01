# API

### Routes
Route | Method  | Short description
------|---------| -----------------
/ws   | GET     | Establishes a WebSocket connection.

### Connect request

After establishing the WebSocket connection, the user needs to send `connect_request` within 10 seconds.
Otherwise, the connection will close through the timeout.

Request parameters:

```json
{
    "type": "connect_request",
    "payload": {
        "agent_label": "",
        "classroom_id": "",
        "token": ""
    }
}
```

Attribute | Type   | Description
----------|--------|------------
type      | string | "connect_request".
agent_label  | string | Agent label.
classroom_id | string | Classroom ID (uuid).
token        | string | JWT token.

#### Successful response

```json
{"type": "connect_success"}
```

#### Unsuccessful response

```json
{ "type": "unrecoverable_session_error", "payload": { "type": "<REASON>", "title": "<REASON>", "status": 422 }}
```
