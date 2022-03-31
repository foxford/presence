use crate::classroom::ClassroomId;
use serde_derive::Serialize;
use sqlx::{postgres::PgQueryResult, types::time::OffsetDateTime, Error, PgConnection};
use std::collections::HashMap;
use svc_agent::AgentId;
use uuid::Uuid;

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct AgentSession {
    pub id: Uuid,
    pub agent_id: AgentId,
    pub classroom_id: ClassroomId,
    pub replica_id: String,
    pub started_at: OffsetDateTime,
}

pub struct InsertQuery<'a> {
    agent_id: &'a AgentId,
    classroom_id: ClassroomId,
    replica_id: String,
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
        let query = sqlx::query_as!(
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
            self.agent_id as &AgentId,
            self.classroom_id as ClassroomId,
            self.replica_id,
            self.started_at
        );

        match query.fetch_one(conn).await {
            Ok(agent_session) => InsertResult::Ok(agent_session),
            Err(sqlx::Error::Database(err)) => {
                if let Some(constraint) = err.constraint() {
                    if constraint == "uniq_classroom_id_agent_id_outdated" {
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
    ids: &'a [Uuid],
    replica_id: &'a str,
}

impl<'a> DeleteQuery<'a> {
    pub fn by_replica(replica_id: &'a str, ids: &'a [Uuid]) -> Self {
        Self { ids, replica_id }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<PgQueryResult> {
        sqlx::query!(
            r#"
            DELETE FROM agent_session
            WHERE id = ANY ($1)
            AND replica_id = $2
            "#,
            self.ids,
            self.replica_id
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

pub struct FindQuery<'a> {
    replica_id: &'a str,
    outdated: bool,
}

pub struct FindResult {
    // pub id: Uuid,
    pub id: (AgentId, ClassroomId),
    // pub agent_id: AgentId,
    // pub classroom_id: ClassroomId,
}

impl<'a> FindQuery<'a> {
    pub fn by_replica(replica_id: &'a str) -> Self {
        Self {
            replica_id,
            outdated: false,
        }
    }

    pub fn outdated(mut self, value: bool) -> Self {
        self.outdated = value;
        self
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<Vec<FindResult>> {
        sqlx::query_as!(
            FindResult,
            r#"
            SELECT
                (agent_id, classroom_id) AS "id!: (AgentId, ClassroomId)"
            FROM agent_session
            WHERE
                replica_id = $1
                AND outdated = $2
            "#,
            self.replica_id,
            self.outdated
        )
        .fetch_all(conn)
        .await
    }
}

pub struct UpdateOutdatedQuery<'a> {
    agent_id: &'a AgentId,
    classroom_id: ClassroomId,
    outdated: bool,
}

impl<'a> UpdateOutdatedQuery<'a> {
    pub fn by_agent_and_classroom(agent_id: &'a AgentId, classroom_id: ClassroomId) -> Self {
        Self {
            agent_id,
            classroom_id,
            outdated: false,
        }
    }

    pub fn outdated(mut self, value: bool) -> Self {
        self.outdated = value;
        self
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<PgQueryResult> {
        sqlx::query!(
            r#"
            UPDATE agent_session
            SET outdated = $1
            WHERE
                agent_id = $2
                AND classroom_id = $3
            "#,
            self.outdated,
            self.agent_id as &AgentId,
            self.classroom_id as ClassroomId
        )
        .execute(conn)
        .await
    }
}

pub struct GetQuery<'a> {
    id: &'a Uuid,
}

impl<'a> GetQuery<'a> {
    pub fn new(id: &'a Uuid) -> Self {
        Self { id }
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
                id = $1
            LIMIT 1
            "#,
            self.id,
        )
        .fetch_one(conn)
        .await
    }
}

// pub struct GetQuery<'a> {
//     agent_id: &'a AgentId,
//     classroom_id: &'a ClassroomId,
//     replica_id: &'a str,
// }
//
// impl<'a> GetQuery<'a> {
//     pub fn new(agent_id: &'a AgentId, classroom_id: &'a ClassroomId, replica_id: &'a str) -> Self {
//         Self {
//             agent_id,
//             classroom_id,
//             replica_id,
//         }
//     }
//
//     pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<Option<AgentSession>> {
//         sqlx::query_as!(
//             AgentSession,
//             r#"
//             SELECT
//                 id,
//                 agent_id AS "agent_id: AgentId",
//                 classroom_id AS "classroom_id: ClassroomId",
//                 replica_id,
//                 started_at
//             FROM agent_session
//             WHERE
//                 agent_id = $1
//                 AND classroom_id = $2
//                 AND replica_id = $3
//             LIMIT 1
//             "#,
//             self.agent_id as &AgentId,
//             self.classroom_id as &ClassroomId,
//             self.replica_id
//         )
//         .fetch_optional(conn)
//         .await
//     }
// }
