use crate::{
    app::{metrics::Metrics, state::AppState},
    authz::AuthzCache,
    session::SessionMap,
};
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
mod local_ip;
mod replica;
mod router;
mod ws;

pub mod metrics;
pub mod nats;
pub mod session_manager;
pub mod state;

pub async fn run(db: PgPool, authz_cache: Option<AuthzCache>) -> Result<()> {
    let replica_label = var("APP_AGENT_LABEL").expect("APP_AGENT_LABEL must be specified");
    let replica_id = replica::register(&db, replica_label).await?;
    info!("Replica successfully registered: {:?}", replica_id);

    let config = crate::config::load().context("Failed to load config")?;
    info!("App config: {:?}", config);

    if let Some(sentry_config) = config.sentry.as_ref() {
        svc_error::extension::sentry::init(sentry_config);
    }

    let authz = svc_authz::ClientMap::new(&config.id, authz_cache, config.authz.clone(), None)
        .context("Error converting authz config to clients")?;

    // A channel for managing agent session via sending commands from WebSocket handler
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<session_manager::Command>();

    let nats_client = {
        info!("Connecting to NATS");
        nats::Client::new(&config.nats.url)?
    };

    let state = AppState::new(
        config.clone(),
        db.clone(),
        authz,
        replica_id,
        cmd_tx,
        nats_client.clone(),
        Metrics::new(),
    );

    // Move hanging sessions from last time to history
    history_manager::move_all_sessions(state.clone(), replica_id)
        .await
        .context("Failed to move all sessions to history")?;

    let metrics_server = svc_utils::metrics::MetricsServer::new(config.metrics_listener_address);

    // For graceful shutdown
    let (shutdown_tx, shutdown_rx) = watch::channel(());

    // Keeps all active sessions on a replica
    let sessions = SessionMap::new();
    let session_manager = session_manager::run(sessions.clone(), cmd_rx, shutdown_rx.clone());

    let router = router::new(state.clone(), config.authn.clone());
    let internal_router = router::new_internal(sessions);

    // Public API
    let mut shutdown_server_rx = shutdown_rx.clone();
    let server = tokio::spawn(
        axum::Server::bind(&config.listener_address)
            .serve(router.into_make_service())
            .with_graceful_shutdown(async move {
                shutdown_server_rx.changed().await.ok();
            }),
    );

    // Internal API
    let mut shutdown_server_rx = shutdown_rx.clone();
    let internal_server = tokio::spawn(
        axum::Server::bind(&config.internal_listener_address)
            .serve(internal_router.into_make_service())
            .with_graceful_shutdown(async move {
                shutdown_server_rx.changed().await.ok();
            }),
    );

    // Waiting for signals for graceful shutdown
    let mut signals_stream = signal_hook_tokio::Signals::new(TERM_SIGNALS)?.fuse();
    let signals = signals_stream.next();
    let _ = signals.await;
    // Initiating graceful shutdown
    shutdown_tx.send(()).ok();

    // Move hanging sessions to history
    if let Err(e) = history_manager::move_all_sessions(state.clone(), replica_id).await {
        error!(error = %e, "Failed to move all sessions to history");
    }

    // Make sure server and session manager, and others are stopped
    if let Err(err) = server.await {
        error!(error = %err, "Failed to await server completion");
    }

    if let Err(err) = internal_server.await {
        error!(error = %err, "Failed to await internal_server completion");
    }

    if let Err(err) = session_manager.await {
        error!(error = %err, "Failed to await session manager completion");
    }

    if let Err(err) = replica::terminate(&db, replica_id).await {
        error!(error = %err, "Failed to terminate replica");
    }

    if let Err(err) = nats_client.shutdown().await {
        error!(error = %err, "Nats client shutdown failed");
    }

    metrics_server.shutdown().await;

    Ok(())
}
