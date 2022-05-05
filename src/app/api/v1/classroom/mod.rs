use crate::{
    app::{
        api::AppResult,
        error::{ErrorExt, ErrorKind},
        metrics::AuthzMeasure,
        state::State,
    },
    authz::AuthzObject,
    authz_hack,
    classroom::ClassroomId,
    db::agent_session,
};
use anyhow::Context;
use axum::{
    extract::Query,
    extract::{Extension, Path},
    response::IntoResponse,
    Json,
};
use serde_derive::Deserialize;
use svc_agent::AgentId;
use svc_authn::Authenticable;
use svc_utils::extractors::AuthnExtractor;

const MAX_LIMIT: usize = 100;

#[derive(Deserialize, Default)]
pub struct Payload {
    offset: Option<usize>,
    limit: Option<usize>,
}

pub async fn list_agents<S: State>(
    Extension(state): Extension<S>,
    Path(classroom_id): Path<ClassroomId>,
    AuthnExtractor(agent_id): AuthnExtractor,
    Query(payload): Query<Payload>,
) -> AppResult {
    do_list_agents(state, classroom_id, agent_id, payload).await
}

async fn do_list_agents<S: State>(
    state: S,
    classroom_id: ClassroomId,
    agent_id: AgentId,
    payload: Payload,
) -> AppResult {
    let account_id = agent_id.as_account_id();
    let object = AuthzObject::new(&["classrooms", &classroom_id.to_string()]).into();

    state
        .authz()
        .authorize(
            authz_hack::remove_unwanted_parts_from_audience(account_id.audience()),
            account_id.clone(),
            object,
            "read".into(),
        )
        .await
        .measure()?;

    let mut conn = state
        .get_conn()
        .await
        .error(ErrorKind::DbConnAcquisitionFailed)?;

    let agents = agent_session::AgentList::new(
        classroom_id,
        payload.offset.unwrap_or(0),
        std::cmp::min(payload.limit.unwrap_or(MAX_LIMIT), MAX_LIMIT),
    )
    .execute(&mut conn)
    .await
    .context("Failed to get list of agents")
    .error(ErrorKind::DbQueryFailed)?;

    Ok(Json(agents).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        classroom::ClassroomId,
        db::agent_session::{self, Agent},
        test_helpers::prelude::*,
    };
    use axum::{body::HttpBody, response::IntoResponse};
    use serde_json::Value;
    use sqlx::types::time::OffsetDateTime;
    use uuid::Uuid;

    const REPLICA_ID: &str = "presence_1";

    #[tokio::test]
    async fn list_agents_unauthorized() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();
        let db_pool = TestDb::new(&postgres.connection_string).await;
        let state = TestState::new(db_pool, TestAuthz::new(), REPLICA_ID);
        let classroom_id: ClassroomId = Uuid::new_v4().into();
        let agent = TestAgent::new("web", "user1", USR_AUDIENCE);

        let resp = do_list_agents(
            state,
            classroom_id,
            agent.agent_id().to_owned(),
            Payload::default(),
        )
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
        let classroom_id: ClassroomId = Uuid::new_v4().into();
        let agent = TestAgent::new("web", "user1", USR_AUDIENCE);

        let _ = {
            let mut conn = db_pool.get_conn().await;

            agent_session::InsertQuery::new(
                agent.agent_id(),
                classroom_id,
                "replica".to_string(),
                OffsetDateTime::now_utc(),
            )
            .execute(&mut conn)
            .await
            .expect("Failed to insert an agent session");

            agent_session::InsertQuery::new(
                agent.agent_id(),
                classroom_id,
                "replica".to_string(),
                OffsetDateTime::now_utc(),
            )
            .outdated(true)
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

        let state = TestState::new(db_pool, authz, REPLICA_ID);

        let resp = do_list_agents(
            state,
            classroom_id,
            agent.agent_id().to_owned(),
            Payload::default(),
        )
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
