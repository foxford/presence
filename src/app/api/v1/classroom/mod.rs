use crate::app::{
    api::AppResult,
    error::{ErrorExt, ErrorKind},
};
use crate::authz::AuthzObject;
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
use svc_authn::Authenticable;
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
    agent_id: AgentId,
) -> AppResult {
    let account_id = agent_id.as_account_id();
    let object = AuthzObject::new(&["classrooms", &classroom_id.to_string()]).into();

    state
        .authz()
        .authorize(
            account_id.audience().to_string(),
            account_id.clone(),
            object,
            "read".into(),
        )
        .await?;

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
    use crate::db::agent_session;
    use crate::db::agent_session::Agent;
    use crate::test_helpers::prelude::*;
    use axum::body::HttpBody;
    use axum::response::IntoResponse;
    use serde_json::Value;
    use sqlx::types::time::OffsetDateTime;
    use uuid::Uuid;

    #[tokio::test]
    async fn list_agents_unauthorized() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();
        let db_pool = TestDb::new(&postgres.connection_string).await;
        let state = TestState::new(db_pool, TestAuthz::new());
        let classroom_id = ClassroomId { 0: Uuid::new_v4() };
        let agent = TestAgent::new("web", "user1", USR_AUDIENCE);

        let resp = do_list_agents(state, classroom_id, agent.agent_id().to_owned())
            .await
            .expect_err("Unexpectedly succeeded")
            .into_response();

        assert_eq!(resp.status(), 403);

        let mut body = resp.into_body();
        let body = body.data().await.unwrap().expect("Failed to get body");
        let json: Value = serde_json::from_slice(&body).expect("Failed to deserialize body");

        assert_eq!(json["type"], "access_denied");
    }

    #[tokio::test]
    async fn list_agents_success() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();
        let db_pool = TestDb::new(&postgres.connection_string).await;
        let classroom_id = ClassroomId { 0: Uuid::new_v4() };
        let agent = TestAgent::new("web", "user1", USR_AUDIENCE);

        let _ = {
            let mut conn = db_pool.get_conn().await;

            agent_session::InsertQuery::new(
                agent.agent_id().to_owned(),
                classroom_id,
                "replica".to_string(),
                OffsetDateTime::now_utc(),
            )
            .execute(&mut conn)
            .await
            .expect("Failed to insert an agent session")
        };

        let mut authz = TestAuthz::new();
        authz.allow(
            agent.account_id(),
            vec!["classrooms", &classroom_id.to_string()],
            "read",
        );

        let state = TestState::new(db_pool, authz);

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
