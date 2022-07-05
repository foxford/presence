# Internal API

### Routes
| Route            | Method | Short description                                  |
|------------------|--------|----------------------------------------------------|
| /api/v1/sessions | DELETE | [Deletes a session](#delete-session) on a replica. |

### Delete session

Request parameters:

```json
{
  "session_key": {
    "agent_id": "",
    "classroom_id": ""
  }
}
```

| Attribute    | Type   | Description          |
|--------------|--------|----------------------|
| agent_id     | string | Agent ID.            |
| classroom_id | string | Classroom ID (uuid). |

#### Successful response

Status: `200`

Response Body:
```json
{"type": "delete_success"}
```

#### Not found

Status: `404`

Response Body:
```json
{"type": "delete_failure", "payload": "not_found"}
```

#### Failed to delete

Status: `422`

Response Body:
```json
{"type": "delete_failure", "payload": "failed_to_delete"}
```

#### Messaging failed

Status: `422`

Response Body:
```json
{"type": "delete_failure", "payload": "messaging_failed"}
```
