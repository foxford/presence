use crate::config::Config;
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{pool::PoolConnection, PgPool, Postgres};
use std::sync::Arc;
use svc_authz::ClientMap as Authz;
use tokio::sync::broadcast::{Receiver, Sender};
use uuid::Uuid;

#[async_trait]
pub trait State: Send + Sync + Clone + 'static {
    fn config(&self) -> &Config;
    fn authz(&self) -> &Authz;
    fn replica_id(&self) -> String;
    fn old_connection_rx(&self) -> Receiver<Uuid>;
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
    sender: Sender<Uuid>,
}

impl AppState {
    pub fn new(
        config: Config,
        db_pool: PgPool,
        authz: Authz,
        replica_id: String,
        sender: Sender<Uuid>,
    ) -> Self {
        Self {
            inner: Arc::new(InnerState {
                config,
                db_pool,
                authz,
                replica_id,
                sender,
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

    fn old_connection_rx(&self) -> Receiver<Uuid> {
        self.inner.sender.subscribe()
    }

    async fn get_conn(&self) -> Result<PoolConnection<Postgres>> {
        self.inner
            .db_pool
            .acquire()
            .await
            .context("Failed to acquire DB connection")
    }
}
