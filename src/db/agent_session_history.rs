use crate::{classroom::ClassroomId, db::agent_session::AgentSession, session::SessionId};
use sqlx::{
    postgres::{types::PgRange, PgQueryResult},
    types::time::OffsetDateTime,
    PgConnection,
};
use std::collections::Bound;
use svc_agent::AgentId;

pub struct AgentSessionHistory {
    pub id: SessionId,
    pub lifetime: PgRange<OffsetDateTime>,
}

pub struct CheckLifetimeOverlapQuery<'a> {
    agent_session: &'a AgentSession,
}

impl<'a> CheckLifetimeOverlapQuery<'a> {
    pub fn new(agent_session: &'a AgentSession) -> Self {
        Self { agent_session }
    }

    pub async fn execute(
        &self,
        conn: &mut PgConnection,
    ) -> sqlx::Result<Option<AgentSessionHistory>> {
        sqlx::query_as!(
            AgentSessionHistory,
            r#"
            SELECT
                id AS "id!: SessionId",
                lifetime AS "lifetime!"
            FROM agent_session_history
            WHERE
                agent_id = $1
                AND classroom_id = $2
                AND lifetime && tstzrange($3, now())
            LIMIT 1
            "#,
            &self.agent_session.agent_id as &AgentId,
            &self.agent_session.classroom_id as &ClassroomId,
            &self.agent_session.started_at
        )
        .fetch_optional(conn)
        .await
    }
}

pub struct InsertQuery<'a> {
    agent_session: &'a AgentSession,
}

impl<'a> InsertQuery<'a> {
    pub fn new(agent_session: &'a AgentSession) -> Self {
        Self { agent_session }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<PgQueryResult> {
        sqlx::query!(
            r#"
            INSERT INTO agent_session_history
                (id, agent_id, classroom_id, lifetime)
            VALUES ($1, $2, $3, tstzrange($4, now()))
            "#,
            &self.agent_session.id as &SessionId,
            &self.agent_session.agent_id as &AgentId,
            &self.agent_session.classroom_id as &ClassroomId,
            &self.agent_session.started_at
        )
        .execute(conn)
        .await
    }
}

pub struct UpdateLifetimeQuery {
    id: SessionId,
    started_at: Bound<OffsetDateTime>,
}

impl UpdateLifetimeQuery {
    pub fn new(id: SessionId, started_at: Bound<OffsetDateTime>) -> Self {
        Self { id, started_at }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<PgQueryResult> {
        sqlx::query!(
            r#"
            UPDATE agent_session_history
            SET lifetime = $2
            WHERE id = $1
            "#,
            self.id as SessionId,
            PgRange::from((self.started_at, Bound::Excluded(OffsetDateTime::now_utc())))
        )
        .execute(conn)
        .await
    }
}

pub struct UpdateLifetimesQuery<'a> {
    replica_id: &'a str,
}

impl<'a> UpdateLifetimesQuery<'a> {
    pub fn by_replica(replica_id: &'a str) -> Self {
        Self { replica_id }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<Vec<SessionId>> {
        sqlx::query_scalar!(
            r#"
            WITH hq AS (
                SELECT
                    s.id,
                    ash.id AS history_id,
                    tstzrange(lower(ash.lifetime), now()) AS new_lifetime
                FROM agent_session s
                    LEFT OUTER JOIN agent_session_history ash
                        ON ash.agent_id = s.agent_id
                            AND ash.classroom_id = s.classroom_id
                            AND ash.lifetime && tstzrange(s.started_at, now())
                WHERE s.replica_id = $1
            )
            UPDATE agent_session_history ash
            SET lifetime = hq.new_lifetime
            FROM hq
            WHERE hq.history_id = ash.id
            RETURNING hq.id AS "id: SessionId"
            "#,
            self.replica_id
        )
        .fetch_all(conn)
        .await
    }
}

pub struct InsertFromAgentSessionQuery<'a> {
    replica_id: &'a str,
    except: &'a [SessionId],
}

impl<'a> InsertFromAgentSessionQuery<'a> {
    pub fn by_replica(replica_id: &'a str) -> Self {
        Self {
            replica_id,
            except: &[],
        }
    }

    pub fn except(self, except: &'a [SessionId]) -> Self {
        Self { except, ..self }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<Vec<SessionId>> {
        let mut select_sql = String::from(
            r#"
            SELECT s.id,
                   s.agent_id,
                   s.classroom_id,
                   tstzrange(s.started_at, now()) AS lifetime
            FROM agent_session s
            WHERE replica_id = $1
        "#,
        );

        if !self.except.is_empty() {
            select_sql = format!("{} AND id != ANY ($2)", select_sql);
        }

        let sql = format!(
            r#"
            WITH sq AS ({})
            INSERT
            INTO agent_session_history
                (id, agent_id, classroom_id, lifetime)
            SELECT *
            FROM sq
            RETURNING id AS "id!: SessionId"
        "#,
            select_sql
        );

        let q = if self.except.is_empty() {
            sqlx::query_scalar(&sql).bind(self.replica_id)
        } else {
            sqlx::query_scalar(&sql)
                .bind(self.replica_id)
                .bind(self.except)
        };

        q.fetch_all(conn).await
    }
}
