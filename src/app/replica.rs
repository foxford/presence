use crate::app::local_ip;
use crate::db;
use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn register(db_pool: &PgPool, label: String) -> Result<Uuid> {
    let ip = local_ip::get_local_ip().context("Failed to get local ip")?;

    let mut conn = db_pool
        .acquire()
        .await
        .context("Failed to acquire DB connection")?;

    let insert_query = db::replica::InsertQuery::new(label, ip)
        .context("Failed to create insert query for replica")?;

    let replica = insert_query
        .execute(&mut conn)
        .await
        .context("Failed to insert replica")?;

    Ok(replica.id)
}

pub async fn terminate(db_pool: &PgPool, id: Uuid) -> Result<()> {
    let mut conn = db_pool
        .acquire()
        .await
        .context("Failed to acquire DB connection")?;

    db::replica::DeleteQuery::new(id)
        .execute(&mut conn)
        .await
        .context("Failed to delete replica")?;

    Ok(())
}
