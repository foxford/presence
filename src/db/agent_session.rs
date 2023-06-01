use crate::{classroom::ClassroomId, session::SessionId};
use serde_derive::Serialize;
use sqlx::{postgres::PgQueryResult, types::time::OffsetDateTime, Error, PgConnection};
use std::collections::HashMap;
use svc_agent::AgentId;
use uuid::Uuid;

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct AgentSession {
    pub id: SessionId,
    pub agent_id: AgentId,
    pub classroom_id: ClassroomId,
    pub replica_id: Uuid,
    pub started_at: OffsetDateTime,
}

pub struct InsertQuery<'a> {
    agent_id: &'a AgentId,
    classroom_id: ClassroomId,
    replica_id: Uuid,
    started_at: OffsetDateTime,
}

pub enum InsertResult {
    Ok(AgentSession),
    Error(Error),
    UniqIdsConstraintError,
}

#[cfg(test)]
impl InsertResult {
    pub fn expect(&self, msg: &str) -> AgentSession {
        match self {
            InsertResult::Ok(s) => s.to_owned(),
            InsertResult::Error(err) => {
                panic!("{}, error: {:?}", msg, err)
            }
            _ => {
                panic!("{}", msg)
            }
        }
    }
}

impl<'a> InsertQuery<'a> {
    pub fn new(
        agent_id: &'a AgentId,
        classroom_id: ClassroomId,
        replica_id: Uuid,
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
        let query = sqlx::query_as!(
            AgentSession,
            r#"
            INSERT INTO agent_session
                (agent_id, classroom_id, replica_id, started_at)
            VALUES ($1, $2, $3, $4)
            RETURNING
                id AS "id: SessionId",
                agent_id AS "agent_id: AgentId",
                classroom_id AS "classroom_id: ClassroomId",
                replica_id,
                started_at
            "#,
            self.agent_id as &AgentId,
            self.classroom_id as ClassroomId,
            self.replica_id,
            self.started_at,
        );

        match query.fetch_one(conn).await {
            Ok(agent_session) => InsertResult::Ok(agent_session),
            Err(Error::Database(err)) => {
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

pub struct DeleteQuery<'a> {
    ids: &'a [SessionId],
    replica_id: Uuid,
}

impl<'a> DeleteQuery<'a> {
    pub fn by_replica(replica_id: Uuid, ids: &'a [SessionId]) -> Self {
        Self { ids, replica_id }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<PgQueryResult> {
        sqlx::query!(
            r#"
            DELETE FROM agent_session
            WHERE id = ANY ($1)
            AND replica_id = $2
            "#,
            self.ids as &[SessionId],
            self.replica_id
        )
        .execute(conn)
        .await
    }
}

#[derive(Serialize)]
pub struct Agent {
    pub id: SessionId,
    pub agent_id: AgentId,
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
                id AS "id: SessionId",
                agent_id AS "agent_id: AgentId"
            FROM agent_session
            WHERE
                classroom_id = $1::uuid
            LIMIT $2
            OFFSET $3
            "#,
            self.classroom_id as ClassroomId,
            self.limit as i32,
            self.offset as i32
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

pub struct AgentCounter<'a> {
    classroom_ids: &'a [ClassroomId],
}

impl<'a> AgentCounter<'a> {
    pub fn new(classroom_ids: &'a [ClassroomId]) -> Self {
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
            self.classroom_ids as &[ClassroomId]
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
    id: SessionId,
}

impl GetQuery {
    pub fn new(id: SessionId) -> Self {
        Self { id }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<AgentSession> {
        sqlx::query_as!(
            AgentSession,
            r#"
            SELECT
                id AS "id: SessionId",
                agent_id AS "agent_id: AgentId",
                classroom_id AS "classroom_id: ClassroomId",
                replica_id,
                started_at
            FROM agent_session
            WHERE
                id = $1
            LIMIT 1
            "#,
            &self.id as &SessionId
        )
        .fetch_one(conn)
        .await
    }
}

pub struct UpdateQuery {
    id: SessionId,
    replica_id: Uuid,
}

impl UpdateQuery {
    pub fn new(id: SessionId, replica_id: Uuid) -> Self {
        Self { id, replica_id }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<PgQueryResult> {
        sqlx::query!(
            r#"
            UPDATE agent_session
            SET replica_id = $2
            WHERE id = $1
            "#,
            &self.id as &SessionId,
            self.replica_id
        )
        .execute(conn)
        .await
    }
}
