use crate::classroom::ClassroomId;
use serde_derive::Serialize;
use sqlx::postgres::PgQueryResult;
use sqlx::types::time::OffsetDateTime;
use sqlx::{Error, PgConnection};
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

pub enum InsertResult {
    Ok(AgentSession),
    Error(Error),
    UniqIdsConstraintError,
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

    pub async fn execute(&self, conn: &mut PgConnection) -> InsertResult {
        let result = sqlx::query_as!(
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
        .await;

        match result {
            Ok(agent_session) => InsertResult::Ok(agent_session),
            Err(sqlx::Error::Database(err)) => {
                if let Some(constraint) = err.constraint() {
                    if constraint == "uniq_classroom_id_agent_id" {
                        return InsertResult::UniqIdsConstraintError;
                    }
                }

                InsertResult::Error(Error::Database(err))
            }
            Err(err) => InsertResult::Error(err),
        }
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
    offset: usize,
    limit: usize,
}

impl AgentList {
    pub fn new(classroom_id: ClassroomId, offset: usize, limit: usize) -> Self {
        Self {
            classroom_id,
            offset,
            limit,
        }
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
            LIMIT $2
            OFFSET $3
            "#,
            self.classroom_id as ClassroomId,
            self.limit as u32,
            self.offset as u32
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

pub struct GetQuery {
    agent_id: AgentId,
    classroom_id: ClassroomId,
    replica_id: String,
}

impl GetQuery {
    pub fn new(agent_id: AgentId, classroom_id: ClassroomId, replica_id: String) -> Self {
        Self {
            agent_id,
            classroom_id,
            replica_id,
        }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<AgentSession> {
        sqlx::query_as!(
            AgentSession,
            r#"
            SELECT
                id,
                agent_id AS "agent_id: AgentId",
                classroom_id AS "classroom_id: ClassroomId",
                replica_id,
                started_at
            FROM agent_session
            WHERE
                agent_id = $1
                AND classroom_id = $2
                AND replica_id = $3
            "#,
            self.agent_id.clone() as AgentId,
            self.classroom_id as ClassroomId,
            self.replica_id,
        )
        .fetch_one(conn)
        .await
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
            FROM agent_session
            WHERE replica_id = $1
            "#,
            self.replica_id,
        )
        .fetch_all(conn)
        .await
    }
}
