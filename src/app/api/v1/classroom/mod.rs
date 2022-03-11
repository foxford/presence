use crate::app::{
    api::AppResult,
    error::{ErrorExt, ErrorKind},
};
use crate::classroom::ClassroomId;
use crate::db::agent_session::AgentList;
use crate::state::State;
use anyhow::Context;
use axum::{
    body,
    extract::{Extension, Path},
};
use http::Response;
use svc_agent::AgentId;
use svc_utils::extractors::AuthnExtractor;

pub async fn list_agents<S: State>(
    Extension(state): Extension<S>,
    Path(classroom_id): Path<ClassroomId>,
    AuthnExtractor(agent_id): AuthnExtractor,
) -> AppResult {
    do_list_agents(state, classroom_id, agent_id).await
}

async fn do_list_agents<S: State>(
    state: S,
    classroom_id: ClassroomId,
    _agent_id: AgentId,
) -> AppResult {
    let mut conn = state
        .get_conn()
        .await
        .error(ErrorKind::DbConnAcquisitionFailed)?;

    let agents = AgentList::new(classroom_id)
        .execute(&mut conn)
        .await
        .context("Failed to get list of agents")
        .error(ErrorKind::DbQueryFailed)?;

    let body = serde_json::to_string(&agents)
        .context("Failed to serialize agents")
        .error(ErrorKind::SerializationFailed)?;

    let resp = Response::builder()
        .body(body::boxed(body::Full::from(body)))
        .context("Failed to build response for agents")
        .error(ErrorKind::ResponseBuildFailed)?;

    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classroom::ClassroomId;
    use crate::db::agent_session::Agent;
    use crate::test_helpers::prelude::*;
    use axum::body::HttpBody;
    use uuid::Uuid;

    #[tokio::test]
    async fn list_agents_test() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();

        let db_pool = TestDb::new(&postgres.connection_string).await;
        let classroom_id = ClassroomId { 0: Uuid::new_v4() };
        let agent = TestAgent::new("web", "user1", USR_AUDIENCE);

        let _ = {
            let mut conn = db_pool.get_conn().await;

            factory::agent_session::AgentSession::new(
                agent.agent_id().to_owned(),
                classroom_id,
                "replica".to_string(),
            )
            .insert(&mut conn)
            .await
            .expect("Failed to insert an agent session")
        };

        let state = TestState::new(db_pool);

        let resp = do_list_agents(state, classroom_id, agent.agent_id().to_owned())
            .await
            .expect("Failed to get list of agents");

        assert_eq!(resp.status(), 200);

        let mut body = resp.into_body();
        let body = body.data().await.unwrap().expect("Failed to get body");
        let object = Agent {
            id: agent.agent_id().to_owned(),
        };

        let json = serde_json::to_string(&vec![object]).expect("Failed to serialize an agent");

        assert_eq!(body, json);
    }
}
