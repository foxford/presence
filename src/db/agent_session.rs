use crate::classroom::ClassroomId;
use serde_derive::Serialize;
use sqlx::PgConnection;
use svc_agent::AgentId;

#[derive(Serialize)]
#[serde(transparent)]
pub struct Agent {
    id: AgentId,
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
                    agent_id AS "id!: AgentId"
                FROM agent_session
                WHERE classroom_id = $1::uuid
            "#,
            self.classroom_id as ClassroomId
        )
        .fetch_all(conn)
        .await
    }
}
