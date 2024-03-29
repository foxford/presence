use crate::{
    app::state::State,
    app::{
        api::AppResult,
        error::{ErrorExt, ErrorKind},
        metrics::AuthzMeasure,
    },
    authz::AuthzObject,
    classroom::ClassroomId,
    db::agent_session,
};
use anyhow::Context;
use axum::{extract::Extension, response::IntoResponse, Json};
use serde_derive::Deserialize;
use svc_agent::AgentId;
use svc_authn::Authenticable;
use svc_utils::extractors::AgentIdExtractor;

#[derive(Deserialize)]
pub struct CounterPayload {
    classroom_ids: Vec<ClassroomId>,
}

pub async fn count_agents<S: State>(
    Extension(state): Extension<S>,
    AgentIdExtractor(agent_id): AgentIdExtractor,
    Json(payload): Json<CounterPayload>,
) -> AppResult {
    do_count_agents(state, agent_id, payload).await
}

async fn do_count_agents<S: State>(
    state: S,
    agent_id: AgentId,
    payload: CounterPayload,
) -> AppResult {
    let account_id = agent_id.as_account_id();
    let object = AuthzObject::new(&["classrooms"]).into();

    state
        .authz()
        .authorize(
            state.config().svc_audience.clone(),
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

    let agents_count = agent_session::AgentCounter::new(&payload.classroom_ids)
        .execute(&mut conn)
        .await
        .context("Failed to count agents")
        .error(ErrorKind::DbQueryFailed)?;

    Ok(Json(agents_count).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        classroom::ClassroomId,
        db::{agent_session, replica},
        test_helpers::prelude::*,
    };
    use axum::{body::HttpBody, response::IntoResponse};
    use serde_json::Value;
    use sqlx::types::time::OffsetDateTime;
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use uuid::Uuid;

    #[tokio::test]
    async fn count_agents_unauthorized() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();
        let db_pool = TestDb::new(&postgres.connection_string).await;
        let classroom_id: ClassroomId = Uuid::new_v4().into();
        let state = TestState::new(db_pool, TestAuthz::new(), Uuid::new_v4());
        let agent = TestAgent::new("web", "user1", USR_AUDIENCE);
        let payload = CounterPayload {
            classroom_ids: vec![classroom_id],
        };

        let resp = do_count_agents(state, agent.agent_id().to_owned(), payload)
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
    async fn count_agents_success() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();
        let db_pool = TestDb::new(&postgres.connection_string).await;
        let classroom_id: ClassroomId = Uuid::new_v4().into();
        let agent_1 = TestAgent::new("web", "user1", USR_AUDIENCE);
        let agent_2 = TestAgent::new("web", "user2", USR_AUDIENCE);

        let replica_id = {
            let mut conn = db_pool.get_conn().await;

            let replica_id = replica::InsertQuery::new(
                "presence-1".into(),
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            )
            .expect("Failed to create insert query for replica")
            .execute(&mut conn)
            .await
            .expect("Failed to insert a replica")
            .id;

            agent_session::InsertQuery::new(
                agent_1.agent_id(),
                classroom_id,
                replica_id,
                OffsetDateTime::now_utc(),
            )
            .execute(&mut conn)
            .await
            .expect("Failed to insert first agent session");

            agent_session::InsertQuery::new(
                agent_2.agent_id(),
                classroom_id,
                replica_id,
                OffsetDateTime::now_utc(),
            )
            .execute(&mut conn)
            .await
            .expect("Failed to insert second agent session");

            replica_id
        };

        let agent = TestAgent::new("web", "user4", USR_AUDIENCE);

        let mut authz = TestAuthz::new();
        authz.set_audience(SVC_AUDIENCE);
        authz.allow(agent.account_id(), vec!["classrooms"], "read");

        let state = TestState::new(db_pool, authz, replica_id);
        let payload = CounterPayload {
            classroom_ids: vec![classroom_id],
        };

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
