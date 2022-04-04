use crate::{
    db::{agent_session, agent_session_history},
    state::State,
};
use anyhow::{anyhow, Result};
use sqlx::Connection;
use uuid::Uuid;

/// Moves all session from the `agent_session` table in `agent_session_history`.
pub async fn move_all_sessions<S: State>(state: S, replica_id: &str) -> Result<()> {
    let mut conn = state
        .get_conn()
        .await
        .map_err(|e| anyhow!("Failed to get db connection: {:?}", e))?;

    let mut tx = conn
        .begin()
        .await
        .map_err(|e| anyhow!("Failed to acquire transaction: {:?}", e))?;

    // Update lifetime in existing histories
    let updated_session_ids =
        agent_session_history::UpdateAllLifetimesQuery::by_replica(replica_id)
            .execute(&mut tx)
            .await
            .map_err(|e| anyhow!("Failed to update lifetime in existing histories: {:?}", e))?;

    let mut session_ids = updated_session_ids
        .iter()
        .map(|s| s.id)
        .collect::<Vec<Uuid>>();

    // Move new sessions to history
    let inserted_session_ids =
        agent_session_history::InsertAllFromAgentSessionQuery::by_replica(replica_id)
            .except(&session_ids)
            .execute(&mut tx)
            .await
            .map_err(|e| anyhow!("Failed to create histories from agent_session: {:?}", e))?;

    session_ids.extend(
        inserted_session_ids
            .iter()
            .map(|s| s.id)
            .collect::<Vec<Uuid>>(),
    );

    // Delete moved sessions
    agent_session::DeleteQuery::by_replica(replica_id, &session_ids)
        .execute(&mut tx)
        .await
        .map_err(|e| anyhow!("Failed to delete agent sessions: {:?}", e))?;

    tx.commit()
        .await
        .map_err(|e| anyhow!("Failed to commit transaction: {:?}", e))?;

    Ok(())
}

pub async fn move_single_session<S: State>(state: S, session_id: &Uuid) -> Result<()> {
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

    agent_session::DeleteQuery::by_replica(&state.replica_id(), &[session.id])
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
    use crate::{classroom::ClassroomId, test_helpers::prelude::*};
    use uuid::Uuid;

    #[tokio::test]
    async fn create_history() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();
        let db_pool = TestDb::new(&postgres.connection_string).await;
        let classroom_id = ClassroomId { 0: Uuid::new_v4() };
        let agent = TestAgent::new("http", "user123", USR_AUDIENCE);

        let session = {
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

        let mut conn = db_pool.get_conn().await;
        move_session(&mut conn, session, SessionKind::Active)
            .await
            .expect("Failed to move session to history");

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
    async fn update_history() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();
        let db_pool = TestDb::new(&postgres.connection_string).await;
        let classroom_id = ClassroomId { 0: Uuid::new_v4() };
        let agent = TestAgent::new("http", "user123", USR_AUDIENCE);

        let session = {
            let mut conn = db_pool.get_conn().await;

            let session = agent_session::InsertQuery::new(
                agent.agent_id().to_owned(),
                classroom_id,
                "replica".to_string(),
                OffsetDateTime::now_utc(),
            )
            .execute(&mut conn)
            .await
            .expect("Failed to insert an agent session");

            agent_session_history::InsertQuery::new(session.clone(), OffsetDateTime::now_utc())
                .execute(&mut conn)
                .await
                .expect("Failed to insert an agent session history");

            session
        };

        let mut conn = db_pool.get_conn().await;
        move_session(&mut conn, session)
            .await
            .expect("Failed to move session to history");

        let history_count =
            factory::agent_session_history::AgentSessionHistoryCounter::count(&mut conn)
                .await
                .expect("Failed to count agent session history");

        assert_eq!(history_count, 1);
    }
}
