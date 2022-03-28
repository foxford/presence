use crate::{app::Command, config::Config};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{pool::PoolConnection, PgPool, Postgres};
use std::sync::Arc;
use svc_authz::ClientMap as Authz;
use tokio::sync::mpsc::UnboundedSender;

#[async_trait]
pub trait State: Send + Sync + Clone + 'static {
    fn config(&self) -> &Config;
    fn authz(&self) -> &Authz;
    fn replica_id(&self) -> String;
    fn cmd_sender(&self) -> UnboundedSender<Command>;
    async fn get_conn(&self) -> Result<PoolConnection<Postgres>>;
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<InnerState>,
}

struct InnerState {
    config: Config,
    db_pool: PgPool,
    authz: Authz,
    replica_id: String,
    cmd_sender: UnboundedSender<Command>,
}

impl AppState {
    pub fn new(
        config: Config,
        db_pool: PgPool,
        authz: Authz,
        replica_id: String,
        cmd_sender: UnboundedSender<Command>,
    ) -> Self {
        Self {
            inner: Arc::new(InnerState {
                config,
                db_pool,
                authz,
                replica_id,
                cmd_sender,
            }),
        }
    }
}

#[async_trait]
impl State for AppState {
    fn config(&self) -> &Config {
        &self.inner.config
    }

    fn authz(&self) -> &Authz {
        &self.inner.authz
    }

    fn replica_id(&self) -> String {
        self.inner.replica_id.clone()
    }

    fn cmd_sender(&self) -> UnboundedSender<Command> {
        self.inner.cmd_sender.clone()
    }

    async fn get_conn(&self) -> Result<PoolConnection<Postgres>> {
        self.inner
            .db_pool
            .acquire()
            .await
            .context("Failed to acquire DB connection")
    }
}
