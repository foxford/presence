use crate::state::State;
use anyhow::{Context, Result};
use futures_util::StreamExt;
use signal_hook::consts::TERM_SIGNALS;
use sqlx::PgPool;
use tracing::{error, info};

mod router;
mod ws;

pub(crate) async fn run(db: PgPool) -> Result<()> {
    let config = crate::config::load().context("Failed to load config")?;
    info!("App config: {:?}", config);

    if let Some(sentry_config) = config.sentry.as_ref() {
        svc_error::extension::sentry::init(sentry_config);
    }

    let state = State::new(config.clone(), db);
    let router = router::new(state);

    // For graceful shutdown
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(
        axum::Server::bind(&config.listener_address.parse()?)
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
