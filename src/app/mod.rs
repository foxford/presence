use crate::{app::session_manager::Command, authz::AuthzCache, state::AppState};
use anyhow::{Context, Result};
use futures_util::StreamExt;
use signal_hook::consts::TERM_SIGNALS;
use sqlx::PgPool;
use std::env::var;
use tokio::sync::{mpsc, watch};
use tracing::{error, info};

mod api;
mod error;
mod history_manager;
mod router;
pub mod session_manager;
mod ws;

pub async fn run(db: PgPool, authz_cache: Option<AuthzCache>) -> Result<()> {
    let replica_id = var("APP_AGENT_LABEL").expect("APP_AGENT_LABEL must be specified");

    let config = crate::config::load().context("Failed to load config")?;
    info!("App config: {:?}", config);

    if let Some(sentry_config) = config.sentry.as_ref() {
        svc_error::extension::sentry::init(sentry_config);
    }

    let authz = svc_authz::ClientMap::new(&config.id, authz_cache, config.authz.clone(), None)
        .context("Error converting authz config to clients")?;

    // A channel for managing agent session via sending commands from WebSocket handler
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();

    let state = AppState::new(config.clone(), db, authz, replica_id.clone(), cmd_tx);
    let router = router::new(state.clone(), config.authn.clone());

    // Move hanging sessions from last time to history
    if let Err(e) = history_manager::move_all_sessions(state.clone(), &replica_id).await {
        error!(error = %e, "Failed to move all sessions to history");
    }

    // For graceful shutdown
    let (shutdown_tx, shutdown_rx) = watch::channel(());

    let mut shutdown_server_rx = shutdown_rx.clone();
    let server = tokio::spawn(
        axum::Server::bind(&config.listener_address)
            .serve(router.into_make_service())
            .with_graceful_shutdown(async move {
                shutdown_server_rx.changed().await.ok();
            }),
    );

    let session_manager = session_manager::run(state.clone(), cmd_rx, shutdown_rx);

    // Waiting for signals for graceful shutdown
    let mut signals_stream = signal_hook_tokio::Signals::new(TERM_SIGNALS)?.fuse();
    let signals = signals_stream.next();
    let _ = signals.await;
    // Initiating graceful shutdown
    shutdown_tx.send(()).ok();

    // Move hanging sessions to history
    if let Err(e) = history_manager::move_all_sessions(state.clone(), &replica_id).await {
        error!(error = %e, "Failed to move all sessions to history");
    }

    // Make sure server and session manager are stopped
    if let Err(err) = server.await {
        error!(error = %err, "Failed to await server completion");
    }

    if let Err(err) = session_manager.await {
        error!(error = %err, "Failed to await session manager completion");
    }

    Ok(())
}
