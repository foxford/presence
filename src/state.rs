use crate::config::Config;
use sqlx::PgPool;

pub(crate) struct State {
    config: Config,
    db_pool: PgPool,
}

impl State {
    pub(crate) fn new(config: Config, db_pool: PgPool) -> Self {
        Self { config, db_pool }
    }
}
