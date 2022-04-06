use crate::session::SessionKey;
use crate::{
    app::session_manager::{Command, Session},
    config::{Config, WebSocketConfig},
    state::{CommandSend, State},
    test_helpers::prelude::*,
};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::{pool::PoolConnection, Postgres};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use svc_authn::AccountId;
use svc_authz::ClientMap as Authz;
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Clone)]
pub struct TestState {
    config: Config,
    db_pool: TestDb,
    authz: Authz,
}

impl TestState {
    pub fn new(db_pool: TestDb, authz: TestAuthz) -> Self {
        Self {
            config: Config {
                id: AccountId::new("presence", SVC_AUDIENCE),
                listener_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
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
        }
    }
}

pub struct TestSender;

impl CommandSend for TestSender {
    fn send(&self, cmd: Command) -> Result<()> {
        match cmd {
            Command::Terminate(_, Some(s)) => {
                s.send(Session::NotFound).expect("failed to send message");
            }
            _ => {}
        }

        Ok(())
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
        "presence_1".to_string()
    }

    fn register_session(&self, _: SessionKey, _: Uuid) -> Result<oneshot::Receiver<()>> {
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
