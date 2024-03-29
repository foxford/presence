use crate::{
    app::{
        api::AppResult,
        error::{ErrorExt, ErrorKind},
        metrics::AuthzMeasure,
        state::State,
    },
    authz::AuthzObject,
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
use serde::Deserialize;
use svc_agent::AgentId;
use svc_authn::Authenticable;
use svc_utils::extractors::AgentIdExtractor;

const MAX_LIMIT: usize = 1_000;

#[derive(Deserialize, Default)]
pub struct Payload {
    sequence_id: Option<usize>,
    limit: Option<usize>,
}

pub async fn list_agents<S: State>(
    Extension(state): Extension<S>,
    Path(classroom_id): Path<ClassroomId>,
    AgentIdExtractor(agent_id): AgentIdExtractor,
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

    let audience = state
        .lookup_known_authz_audience(account_id.audience())
        .unwrap_or(account_id.audience())
        .to_owned();

    state
        .authz()
        .authorize(audience, account_id.clone(), object, "read".into())
        .await
        .measure()?;

    let mut conn = state
        .get_conn()
        .await
        .error(ErrorKind::DbConnAcquisitionFailed)?;

    let agents = agent_session::AgentList::new(
        classroom_id,
        payload.sequence_id.unwrap_or_default(),
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
        db::{
            agent_session::{self, Agent},
            replica,
        },
        test_helpers::prelude::*,
    };
    use axum::{body::HttpBody, response::IntoResponse};
    use serde_json::Value;
    use sqlx::types::time::OffsetDateTime;
    use std::net::{IpAddr, Ipv4Addr};
    use uuid::Uuid;

    #[tokio::test]
    async fn list_agents_unauthorized() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();
        let db_pool = TestDb::new(&postgres.connection_string).await;
        let state = TestState::new(db_pool, TestAuthz::new(), Uuid::new_v4());
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
        let agent1 = TestAgent::new("web", "user1", USR_AUDIENCE);
        let agent2 = TestAgent::new("web", "user2", USR_AUDIENCE);

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

            for agent in [&agent1, &agent2] {
                agent_session::InsertQuery::new(
                    agent.agent_id(),
                    classroom_id,
                    replica_id,
                    OffsetDateTime::now_utc(),
                )
                .execute(&mut conn)
                .await
                .expect("Failed to insert an agent session");
            }

            replica_id
        };

        let mut authz = TestAuthz::new();
        authz.allow(
            agent1.account_id(),
            vec!["classrooms", &classroom_id.to_string()],
            "read",
        );

        let state = TestState::new(db_pool, authz, replica_id);

        let resp = do_list_agents(
            state,
            classroom_id,
            agent1.agent_id().to_owned(),
            Payload {
                sequence_id: Some(1),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to get list of agents");

        assert_eq!(resp.status(), 200);

        let mut body = resp.into_body();
        let body = body.data().await.unwrap().expect("Failed to get body");
        let object = Agent {
            sequence_id: 2.into(),
            agent_id: agent2.agent_id().to_owned(),
        };

        let json = serde_json::to_string(&vec![object]).expect("Failed to serialize an agent");

        assert_eq!(body, json);
    }
}
