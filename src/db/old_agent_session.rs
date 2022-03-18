use crate::classroom::ClassroomId;
use crate::db::agent_session::AgentSession;
use sqlx::postgres::PgQueryResult;
use sqlx::PgConnection;
use svc_agent::AgentId;
use uuid::Uuid;

pub struct InsertQuery {
    agent_session: AgentSession,
}

impl InsertQuery {
    pub fn new(agent_session: AgentSession) -> Self {
        Self { agent_session }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<usize> {
        sqlx::query!(
            r#"
            INSERT INTO old_agent_session
                (id, agent_id, classroom_id, replica_id, started_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            self.agent_session.id as Uuid,
            self.agent_session.agent_id.clone() as AgentId,
            self.agent_session.classroom_id as ClassroomId,
            self.agent_session.replica_id,
            self.agent_session.started_at
        )
        .execute(conn)
        .await
        .map(|r| r.rows_affected() as usize)
    }
}

pub struct GetAllByReplicaQuery {
    replica_id: String,
}

impl GetAllByReplicaQuery {
    pub fn new(replica_id: String) -> Self {
        Self { replica_id }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<Vec<AgentSession>> {
        sqlx::query_as!(
            AgentSession,
            r#"
            SELECT
                id,
                agent_id AS "agent_id: AgentId",
                classroom_id AS "classroom_id: ClassroomId",
                replica_id,
                started_at
            FROM old_agent_session
            WHERE replica_id = $1
            "#,
            self.replica_id,
        )
        .fetch_all(conn)
        .await
    }
}

pub struct DeleteQuery {
    id: Uuid,
}

impl DeleteQuery {
    pub fn new(id: Uuid) -> Self {
        Self { id }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<PgQueryResult> {
        sqlx::query!(
            r#"
            DELETE FROM
                old_agent_session
            WHERE
                id = $1
            "#,
            self.id
        )
        .execute(conn)
        .await
    }
}
