use crate::{
    app::session_manager::Session,
    config::{Config, WebSocketConfig},
    session::{SessionId, SessionKey},
    state::State,
    test_helpers::prelude::*,
};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::{pool::PoolConnection, Postgres};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use svc_authn::AccountId;
use svc_authz::ClientMap as Authz;
use tokio::sync::oneshot;

#[derive(Clone)]
pub struct TestState {
    config: Config,
    db_pool: TestDb,
    authz: Authz,
    replica_id: String,
}

impl TestState {
    pub fn new(db_pool: TestDb, authz: TestAuthz, replica_id: impl Into<String>) -> Self {
        Self {
            config: Config {
                id: AccountId::new("presence", SVC_AUDIENCE),
                listener_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
                metrics_listener_address: SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    8081,
                ),
                sentry: None,
                authn: Default::default(),
                websocket: WebSocketConfig {
                    ping_interval: Default::default(),
                    pong_expiration_interval: Default::default(),
                    authentication_timeout: Default::default(),
                    check_old_connection_interval: Default::default(),
                },
                authz: Default::default(),
                svc_audience: SVC_AUDIENCE.to_string(),
            },
            db_pool,
            authz: authz.into(),
            replica_id: replica_id.into(),
        }
    }
}

#[async_trait]
impl State for TestState {
    fn config(&self) -> &Config {
        &self.config
    }

    fn authz(&self) -> &Authz {
        &self.authz
    }

    fn replica_id(&self) -> String {
        self.replica_id.clone()
    }

    fn register_session(&self, _: SessionKey, _: SessionId) -> Result<oneshot::Receiver<()>> {
        let (_, rx) = oneshot::channel::<()>();
        Ok(rx)
    }

    async fn terminate_session(&self, _: SessionKey, _: bool) -> Result<Session> {
        Ok(Session::NotFound)
    }

    async fn get_conn(&self) -> Result<PoolConnection<Postgres>> {
        let conn = self.db_pool.get_conn().await;
        Ok(conn)
    }
}
