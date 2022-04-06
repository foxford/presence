use crate::{
    app::session_manager::{Command, Session},
    config::Config,
    session::{SessionId, SessionKey},
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{pool::PoolConnection, PgPool, Postgres};
use std::sync::Arc;
use svc_authz::ClientMap as Authz;
use tokio::sync::{mpsc::UnboundedSender, oneshot};

#[async_trait]
pub trait State: Send + Sync + Clone + 'static {
    fn config(&self) -> &Config;
    fn authz(&self) -> &Authz;
    fn replica_id(&self) -> String;
    fn register_session(
        &self,
        session_key: SessionKey,
        session_id: SessionId,
    ) -> Result<oneshot::Receiver<()>>;
    async fn terminate_session(&self, session_key: SessionKey, return_id: bool) -> Result<Session>;
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
}
