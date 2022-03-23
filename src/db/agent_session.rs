use crate::classroom::ClassroomId;
use serde_derive::Serialize;
use sqlx::{
    postgres::PgQueryResult, query, query_as, types::time::OffsetDateTime, Error, PgConnection,
};
use std::{collections::HashMap, fmt::Formatter};
use svc_agent::AgentId;
use uuid::Uuid;

pub enum SessionKind {
    Active,
    Old,
}

impl std::fmt::Display for SessionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            SessionKind::Active => "agent_session",
            SessionKind::Old => "old_agent_session",
        };
        write!(f, "{}", name)
    }
}

#[derive(Clone, Debug, sqlx::FromRow)]
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
    kind: SessionKind,
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

impl InsertQuery {
    pub fn new(
        agent_id: AgentId,
        classroom_id: ClassroomId,
        replica_id: String,
        started_at: OffsetDateTime,
        kind: SessionKind,
    ) -> Self {
        Self {
            agent_id,
            classroom_id,
            replica_id,
            started_at,
            kind,
        }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> InsertResult {
        let result = query_as::<_, AgentSession>(&format!(
            r#"
            INSERT INTO {}
                (agent_id, classroom_id, replica_id, started_at)
            VALUES ($1, $2, $3, $4)
            RETURNING
                id,
                agent_id,
                classroom_id,
                replica_id,
                started_at
            "#,
            self.kind
        ))
        .bind(self.agent_id.clone())
        .bind(self.classroom_id)
        .bind(self.replica_id.clone())
        .bind(self.started_at)
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
    kind: SessionKind,
}

impl DeleteQuery {
    pub fn new(id: Uuid, kind: SessionKind) -> Self {
        Self { id, kind }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<PgQueryResult> {
        query(&format!(r#"DELETE FROM {} WHERE id = $1"#, self.kind))
            .bind(self.id)
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
    kind: SessionKind,
}

impl GetAllByReplicaQuery {
    pub fn new(replica_id: String, kind: SessionKind) -> Self {
        Self { replica_id, kind }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<Vec<AgentSession>> {
        query_as::<_, AgentSession>(&format!(
            r#"
            SELECT
                id,
                agent_id AS "agent_id: AgentId",
                classroom_id AS "classroom_id: ClassroomId",
                replica_id,
                started_at
            FROM {}
            WHERE replica_id = $1
            "#,
            self.kind
        ))
        .bind(self.replica_id.clone())
        .fetch_all(conn)
        .await
    }
}
