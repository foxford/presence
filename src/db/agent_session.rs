use crate::classroom::ClassroomId;
use serde_derive::Serialize;
use sqlx::postgres::PgQueryResult;
use sqlx::types::time::OffsetDateTime;
use sqlx::PgConnection;
use std::collections::HashMap;
use svc_agent::AgentId;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct AgentSession {
    pub id: Uuid,
    pub agent_id: AgentId,
    pub classroom_id: ClassroomId,
    pub replica_id: String,
    pub started_at: OffsetDateTime,
}

pub struct InsertQuery {
    agent_id: AgentId,
    classroom_id: ClassroomId,
    replica_id: String,
    started_at: OffsetDateTime,
}

impl InsertQuery {
    pub fn new(
        agent_id: AgentId,
        classroom_id: ClassroomId,
        replica_id: String,
        started_at: OffsetDateTime,
    ) -> Self {
        Self {
            agent_id,
            classroom_id,
            replica_id,
            started_at,
        }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<AgentSession> {
        sqlx::query_as!(
            AgentSession,
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
            self.agent_id.clone() as AgentId,
            self.classroom_id as ClassroomId,
            self.replica_id,
            self.started_at
        )
        .fetch_one(conn)
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
                agent_session
            WHERE
                id = $1
            "#,
            self.id
        )
        .execute(conn)
        .await
    }
}

#[derive(Serialize)]
#[serde(transparent)]
pub struct Agent {
    pub id: AgentId,
}

pub struct AgentList {
    classroom_id: ClassroomId,
}

impl AgentList {
    pub fn new(classroom_id: ClassroomId) -> Self {
        Self { classroom_id }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<Vec<Agent>> {
        sqlx::query_as!(
            Agent,
            r#"
            SELECT
                agent_id AS "id: AgentId"
            FROM agent_session
            WHERE
                classroom_id = $1::uuid
            "#,
            self.classroom_id as ClassroomId
        )
        .fetch_all(conn)
        .await
    }
}

#[derive(Serialize)]
pub struct AgentCount {
    pub classroom_id: ClassroomId,
    pub count: i64,
}

pub struct AgentCounter {
    // TODO: Use Vec<ClassroomId> instead
    classroom_ids: Vec<Uuid>,
}

impl AgentCounter {
    pub fn new(classroom_ids: Vec<Uuid>) -> Self {
        Self { classroom_ids }
    }

    pub async fn execute(
        &self,
        conn: &mut PgConnection,
    ) -> sqlx::Result<HashMap<ClassroomId, i64>> {
        let query: Vec<AgentCount> = sqlx::query_as!(
            AgentCount,
            r#"
            SELECT
                classroom_id AS "classroom_id: ClassroomId",
                COUNT(agent_id) AS "count!"
            FROM agent_session
            WHERE
                classroom_id = ANY ($1)
            GROUP BY classroom_id
            "#,
            &self.classroom_ids
        )
        .fetch_all(conn)
        .await?;

        let result = query
            .iter()
            .map(|a| (a.classroom_id, a.count))
            .collect::<HashMap<_, _>>();

        Ok(result)
    }
}
