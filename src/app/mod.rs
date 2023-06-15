use crate::{
    app::{
        error::{Error, ErrorKind},
        metrics::Metrics,
        state::AppState,
    },
    authz::AuthzCache,
};
use anyhow::{Context, Result};
use futures_util::StreamExt;
use signal_hook::consts::TERM_SIGNALS;
use sqlx::PgPool;
use std::env::var;
use tokio::sync::{mpsc, watch};
use tracing::{error, info};

mod api;
mod history_manager;
mod http;
mod replica;
mod ws;

pub mod cluster_ip;
pub mod error;
pub mod metrics;
pub mod nats;
pub mod session_manager;
pub mod state;
pub mod util;

pub async fn run(db: PgPool, authz_cache: Option<AuthzCache>) -> Result<()> {
    let replica_label = var("APP_AGENT_LABEL").expect("APP_AGENT_LABEL must be specified");
    let replica_id = replica::register(&db, replica_label).await?;
    info!("Replica successfully registered: {:?}", replica_id);

    let config = crate::config::load()?;
    info!("App config: {:?}", config);

    if let Some(sentry_config) = config.sentry.as_ref() {
        svc_error::extension::sentry::init(sentry_config);
    }

    let authz = svc_authz::ClientMap::new(&config.id, authz_cache, config.authz.clone(), None)
        .context("Error converting authz config to clients")?;

    // A channel for managing agent session via sending commands from WebSocket handler
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<session_manager::SessionCommand>();

    info!("Connecting to NATS");
    let nats_client = nats::Client::new(config.nats.clone()).await?;

    let state = AppState::new(
        config.clone(),
        db.clone(),
        authz,
        replica_id,
        cmd_tx,
        nats_client.clone(),
        Metrics::new(),
    );

    // Move hanging sessions from the last time to history
    history_manager::move_all_sessions(state.clone(), replica_id)
        .await
        .context("Failed to move all sessions to history")?;

    let metrics_server = svc_utils::metrics::MetricsServer::new(config.metrics_listener_address);

    // For graceful shutdown
    let (shutdown_tx, shutdown_rx) = watch::channel(());

    // Keeps all active sessions on a replica
    let session_manager = session_manager::run(
        cmd_rx,
        shutdown_rx.clone(),
        config.websocket.wait_before_close_connection,
    );

    let router = http::router(state.clone(), config.authn.clone());
    let internal_router = http::internal_router(state.clone());

    // Public API
    let mut shutdown_server_rx = shutdown_rx.clone();
    let server = tokio::spawn(
        axum::Server::bind(&config.listener_address)
            .serve(router.into_make_service())
            .with_graceful_shutdown(async move {
                shutdown_server_rx.changed().await.ok();

                let wait_before_close_connection = config.websocket.wait_before_close_connection;
                info!(
                    "Waiting for {}s before closing WebSocket connections",
                    wait_before_close_connection.as_secs()
                );
                tokio::time::sleep(wait_before_close_connection).await;
            }),
    );

    // Internal API
    let mut shutdown_server_rx = shutdown_rx.clone();
    let internal_server = tokio::spawn(
        axum::Server::bind(&config.internal_listener_address)
            .serve(internal_router.into_make_service())
            .with_graceful_shutdown(async move {
                shutdown_server_rx.changed().await.ok();

                // To process requests from another replica during a graceful shutdown
                tokio::time::sleep(config.websocket.wait_before_close_connection).await;
            }),
    );

    // Waiting for signals for graceful shutdown
    let mut signals_stream = signal_hook_tokio::Signals::new(TERM_SIGNALS)?.fuse();
    let signals = signals_stream.next();
    let _ = signals.await;
    // Initiating graceful shutdown
    shutdown_tx.send(()).ok();

    // Make sure session manager, server, and others are stopped
    if let Err(e) = session_manager.await {
        report_error(
            ErrorKind::ShutdownFailed,
            "Failed to await session manager completion",
            e.into(),
        );
    }

    if let Err(e) = server.await {
        report_error(
            ErrorKind::ShutdownFailed,
            "Failed to await server completion",
            e.into(),
        );
    }

    if let Err(e) = internal_server.await {
        report_error(
            ErrorKind::ShutdownFailed,
            "Failed to await internal server completion",
            e.into(),
        );
    }

    // Move hanging sessions to history
    // NOTE: This process should be started after the completion of the internal API
    // Otherwise, presence won't send the `replaced` error to the agent
    if let Err(e) = history_manager::move_all_sessions(state.clone(), replica_id).await {
        report_error(
            ErrorKind::MovingSessionToHistoryFailed,
            "Failed to move all sessions to history",
            e,
        );
    }

    if let Err(e) = replica::terminate(&db, replica_id).await {
        report_error(ErrorKind::ShutdownFailed, "Failed to terminate replica", e);
    }

    if let Err(e) = nats_client.shutdown().await {
        report_error(ErrorKind::ShutdownFailed, "Nats client shutdown failed", e);
    }

    metrics_server.shutdown().await;

    Ok(())
}

fn report_error(kind: ErrorKind, msg: &str, error: anyhow::Error) {
    error!(error = %error, msg);
    Error::new(kind, error).notify_sentry();
}
