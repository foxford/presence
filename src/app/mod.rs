use crate::authz::AuthzCache;
use crate::state::AppState;
use anyhow::{Context, Result};
use futures_util::StreamExt;
use signal_hook::consts::TERM_SIGNALS;
use sqlx::PgPool;
use std::env::var;
use tracing::{error, info};

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

    let state = AppState::new(config.clone(), db, authz, replica_id);
    let router = router::new(state, config.authn.clone());

    // For graceful shutdown
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(
        axum::Server::bind(&config.listener_address)
            .serve(router.into_make_service())
            .with_graceful_shutdown(async {
                shutdown_rx.await.ok();
            }),
    );

    // Waiting for signals for graceful shutdown
    let mut signals_stream = signal_hook_tokio::Signals::new(TERM_SIGNALS)?.fuse();
    let signals = signals_stream.next();
    let _ = signals.await;
    // Initiating graceful shutdown
    let _ = shutdown_tx.send(());

    // Make sure server is stopped
    if let Err(err) = server.await {
        error!("Failed to await server completion, err = {err}");
    }

    Ok(())
}
