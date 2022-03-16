use crate::authz::AuthzCache;
use crate::db;
use crate::state::{AppState, State};
use anyhow::{Context, Result};
use futures_util::StreamExt;
use signal_hook::consts::TERM_SIGNALS;
use sqlx::PgPool;
use std::env::var;
use tokio::select;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{error, info};
use uuid::Uuid;

mod api;
mod error;
mod router;
mod ws;

pub async fn run(db: PgPool, authz_cache: Option<AuthzCache>) -> Result<()> {
    let replica_id = var("REPLICA_ID").expect("REPLICA_ID must be specified");

    let config = crate::config::load().context("Failed to load config")?;
    info!("App config: {:?}", config);

    if let Some(sentry_config) = config.sentry.as_ref() {
        svc_error::extension::sentry::init(sentry_config);
    }

    let authz = svc_authz::ClientMap::new(&config.id, authz_cache, config.authz.clone(), None)
        .context("Error converting authz config to clients")?;

    let (sender, _) = tokio::sync::broadcast::channel::<Uuid>(100); // todo: from config?
    let state = AppState::new(
        config.clone(),
        db,
        authz,
        replica_id.clone(),
        sender.clone(),
    );
    let router = router::new(state.clone(), config.authn.clone());

    let old_connection_handler = tokio::task::spawn(async move {
        let mut conn = state.get_conn().await.expect("Failed to get db connection");

        let mut check_interval = interval(state.config().websocket.check_old_connection_interval);
        check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            select! {
                _ = check_interval.tick() => {
                    match db::old_agent_session::GetQuery::new(replica_id.clone())
                        .execute(&mut conn)
                        .await
                    {
                        Ok(session_ids) => {
                            session_ids.iter().for_each(|s| {
                                if let Err(e) = sender.send(s.id) {
                                    error!(error = %e, "Failed to send session_id");
                                }
                            })
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to get old agent sessions");
                        }
                    }
                }
            }
        }
    });

    // For graceful shutdown
    let (shutdown_server_tx, shutdown_server_rx) = tokio::sync::oneshot::channel::<()>();

    let server = tokio::spawn(
        axum::Server::bind(&config.listener_address)
            .serve(router.into_make_service())
            .with_graceful_shutdown(async {
                shutdown_server_rx.await.ok();
            }),
    );

    // Waiting for signals for graceful shutdown
    let mut signals_stream = signal_hook_tokio::Signals::new(TERM_SIGNALS)?.fuse();
    let signals = signals_stream.next();
    let _ = signals.await;
    // Initiating graceful shutdown
    let _ = shutdown_server_tx.send(());

    // Make sure server and old connection handler are stopped
    if let Err(err) = server.await {
        error!(error = %err, "Failed to await server completion");
    }

    if let Err(err) = old_connection_handler.await {
        error!(error = %err, "Failed to await old connection handler completion");
    }

    Ok(())
}
