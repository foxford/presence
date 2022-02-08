use anyhow::{Context, Result};
use axum::routing::get;
use axum::Router;
use tracing::info;

mod ws;

pub(crate) async fn run() -> Result<()> {
    let config = crate::config::load().context("Failed to load config")?;
    info!("App config: {:?}", config);

    let app = Router::new().route("/ws", get(ws::ws_handler));

    info!("Server is starting...");

    axum::Server::bind(&config.listener_address.parse()?)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
