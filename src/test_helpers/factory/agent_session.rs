use sqlx::PgConnection;

pub struct AgentSessionCounter;

impl AgentSessionCounter {
    pub async fn count(conn: &mut PgConnection) -> sqlx::Result<i64> {
        sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS total
            FROM
                agent_session
            "#,
        )
        .fetch_one(conn)
        .await
        .map(|r| r.total.unwrap_or(0))
    }
}
