use crate::app::api::AppResult;
use crate::app::error::{ErrorExt, ErrorKind};
use crate::db::agent_session::AgentCounter;
use crate::state::State;
use anyhow::Context;
use axum::{body, extract::Extension, Json};
use http::Response;
use serde_derive::Deserialize;
use std::collections::HashMap;
use svc_agent::AgentId;
use svc_utils::extractors::AuthnExtractor;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CounterPayload {
    classroom_ids: Vec<Uuid>,
}

pub async fn count_agents<S: State>(
    Extension(state): Extension<S>,
    AuthnExtractor(agent_id): AuthnExtractor,
    Json(payload): Json<CounterPayload>,
) -> AppResult {
    do_count_agents(state, agent_id, payload).await
}

async fn do_count_agents<S: State>(
    state: S,
    _agent_id: AgentId,
    payload: CounterPayload,
) -> AppResult {
    let mut conn = state
        .get_conn()
        .await
        .error(ErrorKind::DbConnAcquisitionFailed)?;

    let agents_count = AgentCounter::new(payload.classroom_ids)
        .execute(&mut conn)
        .await
        .context("Failed to count agents")
        .error(ErrorKind::DbQueryFailed)?;

    let body = serde_json::to_string(&agents_count)
        .context("Failed to serialize agents count")
        .error(ErrorKind::SerializationFailed)?;

    let resp = Response::builder()
        .body(body::boxed(body::Full::from(body)))
        .context("Failed to build response for agents count")
        .error(ErrorKind::ResponseBuildFailed)?;

    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classroom::ClassroomId;
    use crate::test_helpers::prelude::*;
    use axum::body::HttpBody;

    #[tokio::test]
    async fn count_agents_test() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();

        let db_pool = TestDb::new(&postgres.connection_string).await;
        let classroom_id = ClassroomId { 0: Uuid::new_v4() };
        let agent_1 = TestAgent::new("web", "user1", USR_AUDIENCE);
        let agent_2 = TestAgent::new("web", "user2", USR_AUDIENCE);

        let _ = {
            let mut conn = db_pool.get_conn().await;
            let replica = "replica_id".to_string();

            factory::agent_session::AgentSession::new(
                agent_1.agent_id().to_owned(),
                classroom_id,
                replica.clone(),
            )
            .insert(&mut conn)
            .await
            .expect("Failed to insert first agent session");

            factory::agent_session::AgentSession::new(
                agent_2.agent_id().to_owned(),
                classroom_id,
                replica.clone(),
            )
            .insert(&mut conn)
            .await
            .expect("Failed to insert second agent session")
        };

        let state = TestState::new(db_pool);
        let payload = CounterPayload {
            classroom_ids: vec![classroom_id.0],
        };

        let agent = TestAgent::new("web", "user4", USR_AUDIENCE);

        let resp = do_count_agents(state, agent.agent_id().to_owned(), payload)
            .await
            .expect("Failed to count agents");

        assert_eq!(resp.status(), 200);

        let mut body = resp.into_body();
        let body = body.data().await.unwrap().expect("Failed to get body");

        let mut result: HashMap<ClassroomId, i64> = HashMap::new();
        result.insert(classroom_id, 2);

        let json = serde_json::to_string(&result).expect("Failed to serialize an agent");

        assert_eq!(body, json);
    }
}
