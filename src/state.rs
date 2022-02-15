#![allow(dead_code)]
use crate::config::Config;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct State {
    inner: Arc<InnerState>,
}

struct InnerState {
    config: Config,
    db_pool: PgPool,
}

impl State {
    pub(crate) fn new(config: Config, db_pool: PgPool) -> Self {
        Self {
            inner: Arc::new(InnerState { config, db_pool }),
        }
    }

    pub(crate) fn config(&self) -> &Config {
        &self.inner.config
    }

    pub(crate) fn db_pool(&self) -> &PgPool {
        &self.inner.db_pool
    }
}
