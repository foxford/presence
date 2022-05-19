use crate::{
    app::{
        metrics::Metrics,
        nats::{Client, NatsClient},
        session_manager::{Command, Session},
    },
    config::Config,
    session::{SessionId, SessionKey},
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{pool::PoolConnection, PgPool, Postgres};
use std::sync::Arc;
use svc_authz::ClientMap as Authz;
use tokio::sync::{mpsc::UnboundedSender, oneshot};
use uuid::Uuid;

#[async_trait]
pub trait State: Send + Sync + Clone + 'static {
    fn config(&self) -> &Config;
    fn authz(&self) -> &Authz;
    fn replica_id(&self) -> Uuid;
    fn metrics(&self) -> Metrics;
    fn register_session(
        &self,
        session_key: SessionKey,
        session_id: SessionId,
    ) -> Result<oneshot::Receiver<()>>;
    async fn terminate_session(&self, session_key: SessionKey, return_id: bool) -> Result<Session>;
    async fn get_conn(&self) -> Result<PoolConnection<Postgres>>;
    fn nats_client(&self) -> &dyn NatsClient;
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<InnerState>,
}

struct InnerState {
    config: Config,
    db_pool: PgPool,
    authz: Authz,
    replica_id: Uuid,
    cmd_sender: UnboundedSender<Command>,
    nats_client: Box<dyn NatsClient>,
    metrics: Metrics,
}

impl AppState {
    pub fn new(
        config: Config,
        db_pool: PgPool,
        authz: Authz,
        replica_id: Uuid,
        cmd_sender: UnboundedSender<Command>,
        nats_client: Client,
        metrics: Metrics,
    ) -> Self {
        Self {
            inner: Arc::new(InnerState {
                config,
                db_pool,
                authz,
                replica_id,
                cmd_sender,
                nats_client: Box::new(nats_client) as Box<dyn NatsClient>,
                metrics,
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

    fn replica_id(&self) -> Uuid {
        self.inner.replica_id
    }

    fn metrics(&self) -> Metrics {
        self.inner.metrics.clone()
    }

    fn register_session(
        &self,
        session_key: SessionKey,
        session_id: SessionId,
    ) -> Result<oneshot::Receiver<()>> {
        let (tx, rx) = oneshot::channel::<()>();
        self.inner
            .cmd_sender
            .send(Command::Register(session_key, (session_id, tx)))?;

        Ok(rx)
    }

    async fn terminate_session(&self, session_key: SessionKey, return_id: bool) -> Result<Session> {
        if return_id {
            let (tx, rx) = oneshot::channel::<Session>();
            self.inner
                .cmd_sender
                .send(Command::Terminate(session_key, Some(tx)))?;

            return rx.await.context("failed to receive previous session id");
        }

        self.inner
            .cmd_sender
            .send(Command::Terminate(session_key, None))?;

        Ok(Session::Skip)
    }

    async fn get_conn(&self) -> Result<PoolConnection<Postgres>> {
        self.inner
            .db_pool
            .acquire()
            .await
            .context("Failed to acquire DB connection")
    }

    fn nats_client(&self) -> &dyn NatsClient {
        self.inner.nats_client.as_ref()
    }
}
