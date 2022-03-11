use crate::classroom::ClassroomId;
use crate::db::agent_session::AgentSession as Object;
use sqlx::{types::time::OffsetDateTime, PgConnection};
use svc_agent::AgentId;

pub struct AgentSession {
    agent_id: AgentId,
    classroom_id: ClassroomId,
    replica_id: String,
    started_at: OffsetDateTime,
}

impl AgentSession {
    pub fn new(agent_id: AgentId, classroom_id: ClassroomId, replica_id: String) -> Self {
        Self {
            agent_id,
            classroom_id,
            replica_id,
            started_at: OffsetDateTime::now_utc(),
        }
    }

    pub async fn insert(self, conn: &mut PgConnection) -> sqlx::Result<Object> {
        sqlx::query_as!(
            Object,
            r#"
            INSERT INTO agent_session
                (agent_id, classroom_id, replica_id, started_at)
            VALUES ($1, $2, $3, $4)
            RETURNING
                id,
                agent_id AS "agent_id: AgentId",
                classroom_id AS "classroom_id: ClassroomId",
                replica_id,
                started_at
            "#,
            self.agent_id as AgentId,
            self.classroom_id as ClassroomId,
            self.replica_id,
            self.started_at
        )
        .fetch_one(conn)
        .await
    }
}
