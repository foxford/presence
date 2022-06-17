use crate::{
    app::{
        api::v1::session::{DeletePayload, Response},
        state::State,
    },
    db,
    session::SessionKey,
};
use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::info;
use uuid::Uuid;

pub async fn register(db_pool: &PgPool, label: String) -> Result<Uuid> {
    let ip = local_ip_address::local_ip().context("Failed to get local ip")?;
    info!("Replica IP: {}", ip);

    let mut conn = db_pool
        .acquire()
        .await
        .context("Failed to acquire DB connection")?;

    let insert_query = db::replica::InsertQuery::new(label, ip)
        .context("Failed to create insert query for replica")?;

    let replica = insert_query
        .execute(&mut conn)
        .await
        .context("Failed to insert a replica")?;

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

pub async fn close_connection<S: State>(
    state: S,
    replica_id: Uuid,
    session_key: SessionKey,
) -> Result<Response> {
    let mut conn = state.get_conn().await?;

    let replica_ip = db::replica::GetIpQuery::new(replica_id)
        .execute(&mut conn)
        .await
        .context("Failed to get replica ip")?;

    let replica_ip = replica_ip.ip.ip();

    let url = format!(
        "http://{}:{}/api/internal/session",
        replica_ip,
        state.config().internal_listener_address.port()
    );

    info!(replica_id = %replica_id, replica_ip = %replica_ip, "Trying to close connection on another replica");

    let payload = DeletePayload { session_key };
    let resp = reqwest::Client::new()
        .delete(url)
        .json(&payload)
        .send()
        .await?;

    let resp = resp.json::<Response>().await?;

    Ok(resp)
}
