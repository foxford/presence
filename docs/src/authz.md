# Authorization

In order to authorize an **action** performed by a **subject** to an **object**, the application sends a `POST` request to the authorization endpoint.

**Example**

```json
{
    "subject": {
        "namespace": "iam.example.org",
        "value": "123e4567-e89b-12d3-a456-426655440000"
    },
    "object": {
        "namespace": "presence.svc.example.org",
        "value": ["classrooms", "123e4567-e89b-12d3-a456-426655440000"]
    },
    "action": "read"
}
``` 

Subject's namespace and account label are retrieved from `Authorization` header `Bearer ${token}` token of HTTP request.

URI of authorization endpoint, object and anonymous namespaces are configured through the application configuration file.

Possible values for `OBJECT` and `ACTION`:

Object                       | Action  | Description
-----------------------------|---------| -----------
["classrooms"]               | read    | A service counts online agents.
["classrooms", CLASSROOM_ID] | read    | An user reads information about the number of online agents in the classroom.
["classrooms", CLASSROOM_ID] | connect | An user connects to the classroom.
