use crate::classroom::ClassroomId;
use crate::db::agent_session::AgentSession;
use sqlx::postgres::types::PgRange;
use sqlx::postgres::PgQueryResult;
use sqlx::types::time::OffsetDateTime;
use sqlx::PgConnection;
use std::collections::Bound;
use svc_agent::AgentId;
use uuid::Uuid;

pub struct AgentSessionHistory {
    pub id: Uuid,
    pub lifetime: PgRange<OffsetDateTime>,
}

pub struct CheckLifetimeOverlapQuery {
    agent_session: AgentSession,
    finished_at: OffsetDateTime,
}

impl CheckLifetimeOverlapQuery {
    pub fn new(agent_session: AgentSession, finished_at: OffsetDateTime) -> Self {
        Self {
            agent_session,
            finished_at,
        }
    }

    pub async fn execute(
        &self,
        conn: &mut PgConnection,
    ) -> sqlx::Result<Option<AgentSessionHistory>> {
        sqlx::query_as!(
            AgentSessionHistory,
            r#"
            SELECT
                id, lifetime
            FROM agent_session_history
            WHERE
                agent_id = $1
                AND classroom_id = $2
                AND lifetime && $3
            "#,
            self.agent_session.agent_id.clone() as AgentId,
            self.agent_session.classroom_id as ClassroomId,
            PgRange::from(self.agent_session.started_at..self.finished_at)
        )
        .fetch_optional(conn)
        .await
    }
}

pub struct InsertQuery {
    agent_session: AgentSession,
    finished_at: OffsetDateTime,
}

impl InsertQuery {
    pub fn new(agent_session: AgentSession, finished_at: OffsetDateTime) -> Self {
        Self {
            agent_session,
            finished_at,
        }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<PgQueryResult> {
        sqlx::query!(
            r#"
            INSERT INTO agent_session_history
                (agent_id, classroom_id, lifetime)
            VALUES ($1, $2, $3)
            "#,
            self.agent_session.agent_id.clone() as AgentId,
            self.agent_session.classroom_id as ClassroomId,
            PgRange::from(self.agent_session.started_at..self.finished_at)
        )
        .execute(conn)
        .await
    }
}

pub struct UpdateLifetimeQuery {
    id: Uuid,
    started_at: Bound<OffsetDateTime>,
    finished_at: Bound<OffsetDateTime>,
}

impl UpdateLifetimeQuery {
    pub fn new(
        id: Uuid,
        started_at: Bound<OffsetDateTime>,
        finished_at: Bound<OffsetDateTime>,
    ) -> Self {
        Self {
            id,
            started_at,
            finished_at,
        }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<PgQueryResult> {
        sqlx::query!(
            r#"
            UPDATE agent_session_history
            SET lifetime = $2
            WHERE id = $1
            "#,
            self.id,
            PgRange::from((self.started_at, self.finished_at))
        )
        .execute(conn)
        .await
    }
}
