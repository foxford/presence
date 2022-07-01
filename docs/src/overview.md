# Overview

Service `presence` does several things:
1. Counts the number of agents in classrooms via `WebSocket` connections and gives this information through the API;
2. Engages in forwarding messages from `Nats` from backend services such as `Event` to frontend applications via WebSocket connections.
