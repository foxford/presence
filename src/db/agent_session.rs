use crate::classroom::ClassroomId;
use serde_derive::Serialize;
use sqlx::PgConnection;
use svc_agent::AgentId;
use uuid::Uuid;

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
    pub count: Option<i64>,
}

pub struct AgentCounter {
    // TODO: Use Vec<ClassroomId> instead
    classroom_ids: Vec<Uuid>,
}

impl AgentCounter {
    pub fn new(classroom_ids: Vec<Uuid>) -> Self {
        Self { classroom_ids }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<Vec<AgentCount>> {
        sqlx::query_as!(
            AgentCount,
            r#"
            SELECT
                classroom_id AS "classroom_id: ClassroomId",
                COUNT(agent_id) AS count
            FROM agent_session
            WHERE
                classroom_id = ANY ($1)
            GROUP BY classroom_id
            ORDER BY count DESC
            "#,
            &self.classroom_ids
        )
        .fetch_all(conn)
        .await
    }
}
