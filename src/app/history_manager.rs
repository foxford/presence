use crate::{
    app::state::State,
    db::{agent_session, agent_session_history},
    session::SessionId,
};
use anyhow::{anyhow, Result};
use sqlx::Connection;
use uuid::Uuid;

/// Moves all session from the `agent_session` table in `agent_session_history`.
pub async fn move_all_sessions<S: State>(state: S, replica_id: Uuid) -> Result<()> {
    let mut conn = state
        .get_conn()
        .await
        .map_err(|e| anyhow!("failed to get db connection: {:?}", e))?;

    let mut tx = conn
        .begin()
        .await
        .map_err(|e| anyhow!("failed to acquire transaction: {:?}", e))?;

    // Update lifetime in existing histories
    let mut session_ids = agent_session_history::UpdateLifetimesQuery::by_replica(replica_id)
        .execute(&mut tx)
        .await
        .map_err(|e| anyhow!("failed to update lifetime in existing histories: {:?}", e))?;

    // Move new sessions to history
    let inserted_session_ids =
        agent_session_history::InsertFromAgentSessionQuery::by_replica(replica_id)
            .except(&session_ids)
            .execute(&mut tx)
            .await
            .map_err(|e| anyhow!("failed to create histories from agent_session: {:?}", e))?;

    session_ids.extend(inserted_session_ids.into_iter());

    // Delete moved sessions
    agent_session::DeleteQuery::by_replica(replica_id, &session_ids)
        .execute(&mut tx)
        .await
        .map_err(|e| anyhow!("failed to delete agent sessions: {:?}", e))?;

    tx.commit()
        .await
        .map_err(|e| anyhow!("failed to commit transaction: {:?}", e))?;

    Ok(())
}

pub async fn move_single_session<S: State>(state: S, session_id: SessionId) -> Result<()> {
    let mut conn = state
        .get_conn()
        .await
        .map_err(|e| anyhow!("Failed to get db connection: {:?}", e))?;

    let mut tx = conn
        .begin()
        .await
        .map_err(|e| anyhow!("Failed to acquire transaction: {:?}", e))?;

    let session = agent_session::GetQuery::new(session_id)
        .execute(&mut tx)
        .await
        .map_err(|e| anyhow!("Failed to get session: {:?}", e))?;

    let session_history = agent_session_history::CheckLifetimeOverlapQuery::new(&session)
        .execute(&mut tx)
        .await
        .map_err(|e| {
            anyhow!(
                "Failed to check agent_session_history lifetime overlap: {:?}",
                e
            )
        })?;

    match session_history {
        Some(history) => {
            agent_session_history::UpdateLifetimeQuery::new(history.id, history.lifetime.start)
                .execute(&mut tx)
                .await
                .map_err(|e| anyhow!("Failed to update agent_session_history lifetime: {:?}", e))?;
        }
        None => {
            agent_session_history::InsertQuery::new(&session)
                .execute(&mut tx)
                .await
                .map_err(|e| anyhow!("Failed to create agent_session_history: {:?}", e))?;
        }
    }

    agent_session::DeleteQuery::by_replica(state.replica_id(), &[session.id])
        .execute(&mut tx)
        .await
        .map_err(|e| anyhow!("Failed to delete agent_session: {:?}", e))?;

    tx.commit()
        .await
        .map_err(|e| anyhow!("Failed to commit transaction: {:?}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{classroom::ClassroomId, db::replica, test_helpers::prelude::*};
    use sqlx::types::time::OffsetDateTime;
    use std::net::{IpAddr, Ipv4Addr};
    use uuid::Uuid;

    mod move_all_sessions {
        use super::*;
        use std::ops::Add;
        use std::time::Duration;

        #[tokio::test]
        async fn create_new_histories() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let classroom_id_1: ClassroomId = Uuid::new_v4().into();
            let classroom_id_2: ClassroomId = Uuid::new_v4().into();
            let agent_1 = TestAgent::new("http", "user1", USR_AUDIENCE);
            let agent_2 = TestAgent::new("http", "user2", USR_AUDIENCE);

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
                    classroom_id_1,
                    replica_id,
                    OffsetDateTime::now_utc(),
                )
                .execute(&mut conn)
                .await
                .expect("Failed to insert first agent session");

                agent_session::InsertQuery::new(
                    agent_2.agent_id(),
                    classroom_id_2,
                    replica_id,
                    OffsetDateTime::now_utc(),
                )
                .execute(&mut conn)
                .await
                .expect("Failed to insert second agent session");

                replica_id
            };

            let state = TestState::new(db_pool.clone(), TestAuthz::new(), replica_id);
            move_all_sessions(state, replica_id)
                .await
                .expect("Failed to move all sessions to history");

            let mut conn = db_pool.get_conn().await;
            let agents_count = factory::agent_session::AgentSessionCounter::count(&mut conn)
                .await
                .expect("Failed to count agent session");

            let history_count =
                factory::agent_session_history::AgentSessionHistoryCounter::count(&mut conn)
                    .await
                    .expect("Failed to count agent session history");

            assert_eq!(agents_count, 0);
            assert_eq!(history_count, 2);
        }

        #[tokio::test]
        async fn update_and_create_histories() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let classroom_id_1: ClassroomId = Uuid::new_v4().into();
            let classroom_id_2: ClassroomId = Uuid::new_v4().into();
            let agent_1 = TestAgent::new("http", "user1", USR_AUDIENCE);
            let agent_2 = TestAgent::new("http", "user2", USR_AUDIENCE);

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

                let past = OffsetDateTime::now_utc() - Duration::from_secs(120 * 60);

                let session = agent_session::AgentSession {
                    id: 1.into(),
                    agent_id: agent_1.agent_id().to_owned(),
                    classroom_id: classroom_id_1,
                    replica_id,
                    started_at: past,
                };

                agent_session_history::InsertQuery::new(&session)
                    .execute(&mut conn)
                    .await
                    .expect("failed to insert an agent session");

                agent_session::InsertQuery::new(
                    agent_1.agent_id(),
                    classroom_id_1,
                    replica_id,
                    past.add(Duration::from_secs(60 * 60)),
                )
                .execute(&mut conn)
                .await
                .expect("Failed to insert first agent session");

                agent_session::InsertQuery::new(
                    agent_2.agent_id(),
                    classroom_id_2,
                    replica_id,
                    OffsetDateTime::now_utc(),
                )
                .execute(&mut conn)
                .await
                .expect("Failed to insert second agent session");

                replica_id
            };

            let state = TestState::new(db_pool.clone(), TestAuthz::new(), replica_id);
            move_all_sessions(state, replica_id)
                .await
                .expect("Failed to move all sessions to history");

            let mut conn = db_pool.get_conn().await;
            let agents_count = factory::agent_session::AgentSessionCounter::count(&mut conn)
                .await
                .expect("Failed to count agent session");

            let history_count =
                factory::agent_session_history::AgentSessionHistoryCounter::count(&mut conn)
                    .await
                    .expect("Failed to count agent session history");

            assert_eq!(agents_count, 0);
            assert_eq!(history_count, 2);
        }
    }

    mod move_single_session {
        use super::*;

        #[tokio::test]
        async fn create_new_history() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let classroom_id: ClassroomId = Uuid::new_v4().into();
            let agent = TestAgent::new("http", "user123", USR_AUDIENCE);

            let (session, replica_id) = {
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

                let session = agent_session::InsertQuery::new(
                    agent.agent_id(),
                    classroom_id,
                    replica_id,
                    OffsetDateTime::now_utc(),
                )
                .execute(&mut conn)
                .await
                .expect("Failed to insert an agent session");

                (session, replica_id)
            };

            let state = TestState::new(db_pool.clone(), TestAuthz::new(), replica_id);
            move_single_session(state, session.id)
                .await
                .expect("Failed to move session to history");

            let mut conn = db_pool.get_conn().await;
            let agents_count = factory::agent_session::AgentSessionCounter::count(&mut conn)
                .await
                .expect("Failed to count agent session");

            let history_count =
                factory::agent_session_history::AgentSessionHistoryCounter::count(&mut conn)
                    .await
                    .expect("Failed to count agent session history");

            assert_eq!(agents_count, 0);
            assert_eq!(history_count, 1);
        }

        #[tokio::test]
        async fn update_existing_history() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let classroom_id: ClassroomId = Uuid::new_v4().into();
            let agent = TestAgent::new("http", "user123", USR_AUDIENCE);

            let (session, replica_id) = {
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

                let session = agent_session::InsertQuery::new(
                    agent.agent_id(),
                    classroom_id,
                    replica_id,
                    OffsetDateTime::now_utc(),
                )
                .execute(&mut conn)
                .await
                .expect("Failed to insert an agent session");

                agent_session_history::InsertQuery::new(&session)
                    .execute(&mut conn)
                    .await
                    .expect("Failed to insert an agent session history");

                (session, replica_id)
            };

            let state = TestState::new(db_pool.clone(), TestAuthz::new(), replica_id);
            move_single_session(state, session.id)
                .await
                .expect("Failed to move session to history");

            let mut conn = db_pool.get_conn().await;
            let history_count =
                factory::agent_session_history::AgentSessionHistoryCounter::count(&mut conn)
                    .await
                    .expect("Failed to count agent session history");

            assert_eq!(history_count, 1);
        }
    }
}
