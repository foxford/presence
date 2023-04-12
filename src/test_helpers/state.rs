use crate::{
    app::{
        metrics::Metrics,
        nats::NatsClient,
        session_manager::{ConnectionCommand, DeleteSession, TerminateSession},
        state::State,
    },
    classroom::ClassroomId,
    config::{Config, NatsConfig, WebSocketConfig},
    session::{SessionId, SessionKey},
    test_helpers::prelude::*,
};
use anyhow::Result;
use async_trait::async_trait;
use nats::Message;
use sqlx::{pool::PoolConnection, Postgres};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};
use svc_authn::AccountId;
use svc_authz::ClientMap as Authz;
use tokio::sync::{broadcast::Receiver, oneshot};
use uuid::Uuid;

#[derive(Clone)]
pub struct TestState {
    config: Config,
    db_pool: TestDb,
    authz: Authz,
    replica_id: Uuid,
    nats_client: Arc<dyn NatsClient>,
}

impl TestState {
    pub fn new(db_pool: TestDb, authz: TestAuthz, replica_id: Uuid) -> Self {
        Self {
            config: Config {
                id: AccountId::new("presence", SVC_AUDIENCE),
                listener_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3000),
                metrics_listener_address: SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    3001,
                ),
                internal_listener_address: SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    3002,
                ),
                sentry: None,
                authn: Default::default(),
                websocket: WebSocketConfig {
                    ping_interval: Default::default(),
                    pong_expiration_interval: Default::default(),
                    authentication_timeout: Default::default(),
                    wait_before_close_connection: Default::default(),
                },
                authz: Default::default(),
                svc_audience: SVC_AUDIENCE.to_string(),
                nats: Some(NatsConfig {
                    url: "localhost:1234".into(),
                }),
            },
            db_pool,
            authz: authz.into(),
            replica_id,
            nats_client: Arc::new(TestNatsClient {}) as Arc<dyn NatsClient>,
        }
    }
}

struct TestNatsClient;

#[async_trait]
impl NatsClient for TestNatsClient {
    async fn subscribe(&self, _: ClassroomId) -> Result<Receiver<Message>> {
        todo!()
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

    fn replica_id(&self) -> Uuid {
        self.replica_id
    }

    fn metrics(&self) -> Metrics {
        todo!()
    }

    fn register_session(
        &self,
        _: SessionKey,
        _: SessionId,
    ) -> Result<oneshot::Receiver<ConnectionCommand>> {
        let (_, rx) = oneshot::channel::<ConnectionCommand>();
        Ok(rx)
    }

    async fn terminate_session(&self, _: SessionKey) -> Result<TerminateSession> {
        Ok(TerminateSession::NotFound)
    }

    async fn delete_session(&self, _: SessionKey) -> Result<DeleteSession> {
        Ok(DeleteSession::NotFound)
    }

    async fn get_conn(&self) -> Result<PoolConnection<Postgres>> {
        let conn = self.db_pool.get_conn().await;
        Ok(conn)
    }

    fn nats_client(&self) -> Option<&dyn NatsClient> {
        Some(self.nats_client.as_ref())
    }
}
