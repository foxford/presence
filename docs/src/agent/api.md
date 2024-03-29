# API

All routes expect json payloads.

### Routes
| Route                                   | Method | Short description                                                                                      |
|-----------------------------------------|--------|--------------------------------------------------------------------------------------------------------|
| /api/v1/classrooms/:classroom_id/agents | GET    | [Get the number of online agents](#get-the-number-of-online-agents-in-the-classroom) in the classroom. |
| /api/v1/counters/agent                  | POST   | [Counts](#count-online-agents) online agents in classrooms.                                            |

### Get the number of online agents

Request parameters:

| Attribute    | Type | Optional | Description                                                               |
|--------------|------|----------|---------------------------------------------------------------------------|
| classroom_id | uuid |          | Classroom ID.                                                             |
| sequence_id  | int  | +        | `sequence_id` of the last seen agent on the previous page (Default: `0`). |
| limit        | int  | +        | Pagination limit (Default: `1000`).                                       |

Response status: `200`

Response Body:

| Type          | Description                                     |
|---------------|-------------------------------------------------|
| array[object] | An array of objects with SessionId and AgentId. |

Example:
```json
[
    {
        "id": 1,
        "agent_id": "web.Z2lkOi8vc3RvZWdlL1VzZXI6OlB1cGlsLzIyNDM1MTg=.testing01.usr.foxford.ru"
    }
]
```
### Count online agents

Request parameters:

| Attribute     | Type   | Optional | Description     |
|---------------|--------|----------|-----------------|
| classroom_ids | [uuid] |          | Classroom ID's. |

Response status: `200`

Response Body:

| Type          | Description                                                        |
|---------------|--------------------------------------------------------------------|
| {string: int} | A JSON object with classroom ID's and the number of agents in them |

Example:

```json
{ "0d8fc826-85a0-433f-97fa-df748267787f": 10 }
```



