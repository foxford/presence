use crate::config::Config;
use anyhow::{Context, Result};
use sqlx::pool::PoolConnection;
use sqlx::{PgPool, Postgres};
use std::sync::Arc;

#[derive(Clone)]
pub struct State {
    inner: Arc<InnerState>,
}

struct InnerState {
    config: Config,
    db_pool: PgPool,
}

impl State {
    pub fn new(config: Config, db_pool: PgPool) -> Self {
        Self {
            inner: Arc::new(InnerState { config, db_pool }),
        }
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    pub async fn get_conn(&self) -> Result<PoolConnection<Postgres>> {
        self.inner
            .db_pool
            .acquire()
            .await
            .context("Failed to acquire DB connection")
    }
}
