use crate::state::State;
use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::info;

mod router;
mod ws;

// TODO: router
// TODO: graceful shutdown
// TODO: handle signals
// TODO: add metrics

pub(crate) async fn run(db: PgPool) -> Result<()> {
    let config = crate::config::load().context("Failed to load config")?;
    info!("App config: {:?}", config);

    if let Some(sentry_config) = config.sentry.as_ref() {
        svc_error::extension::sentry::init(sentry_config);
    }

    let state = State::new(config.clone(), db);
    let router = router::new(state);

    axum::Server::bind(&config.listener_address.parse()?)
        .serve(router.into_make_service())
        .await?;

    Ok(())
}
