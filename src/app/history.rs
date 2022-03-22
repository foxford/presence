use crate::db::{
    agent_session::{self, AgentSession},
    agent_session_history, old_agent_session,
};
use anyhow::{anyhow, Result};
use sqlx::pool::PoolConnection;
use sqlx::{types::time::OffsetDateTime, Connection, Postgres};
use std::ops::Bound;

pub async fn move_session(
    conn: &mut PoolConnection<Postgres>,
    session: AgentSession,
) -> Result<()> {
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| anyhow!("Failed to acquire transaction: {:?}", e))?;

    let now = OffsetDateTime::now_utc();
    let session_history =
        agent_session_history::CheckLifetimeOverlapQuery::new(session.clone(), now)
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
            agent_session_history::UpdateLifetimeQuery::new(
                history.id,
                history.lifetime.start,
                Bound::Excluded(now),
            )
            .execute(&mut tx)
            .await
            .map_err(|e| anyhow!("Failed to update agent_session_history lifetime: {:?}", e))?;
        }
        None => {
            agent_session_history::InsertQuery::new(session.clone(), now)
                .execute(&mut tx)
                .await
                .map_err(|e| anyhow!("Failed to create agent_session_history: {:?}", e))?;
        }
    }

    agent_session::DeleteQuery::new(session.id)
        .execute(&mut tx)
        .await
        .map_err(|e| anyhow!("Failed to delete agent_session: {:?}", e))?;

    tx.commit()
        .await
        .map_err(|e| anyhow!("Failed to commit transaction: {:?}", e))?;

    Ok(())
}

pub async fn move_old_session(
    conn: &mut PoolConnection<Postgres>,
    session: AgentSession,
) -> Result<()> {
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| anyhow!("Failed to acquire transaction: {:?}", e))?;

    agent_session_history::InsertQuery::new(session.clone(), OffsetDateTime::now_utc())
        .execute(&mut tx)
        .await
        .map_err(|e| anyhow!("Failed to create agent_session_history: {:?}", e))?;

    old_agent_session::DeleteQuery::new(session.id)
        .execute(&mut tx)
        .await
        .map_err(|e| anyhow!("Failed to delete old_agent_session: {:?}", e))?;

    tx.commit()
        .await
        .map_err(|e| anyhow!("Failed to commit transaction: {:?}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classroom::ClassroomId;
    use crate::db::agent_session::InsertResult;
    use crate::test_helpers::prelude::*;
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

            match agent_session::InsertQuery::new(
                agent.agent_id().to_owned(),
                classroom_id,
                "replica".to_string(),
                OffsetDateTime::now_utc(),
            )
            .execute(&mut conn)
            .await
            {
                InsertResult::Ok(s) => s,
                _ => {
                    panic!("Failed to insert an agent session")
                }
            }
        };

        let mut conn = db_pool.get_conn().await;
        move_session(&mut conn, session)
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

            let session = match agent_session::InsertQuery::new(
                agent.agent_id().to_owned(),
                classroom_id,
                "replica".to_string(),
                OffsetDateTime::now_utc(),
            )
            .execute(&mut conn)
            .await
            {
                InsertResult::Ok(s) => s,
                _ => {
                    panic!("Failed to insert an agent session")
                }
            };

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
