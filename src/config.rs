use serde_derive::Deserialize;
use std::{net::SocketAddr, time::Duration};
use svc_authn::{jose::ConfigMap as AuthnConfig, AccountId};
use svc_authz::ConfigMap as Authz;
use svc_error::extension::sentry::Config as SentryConfig;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub id: AccountId,
    pub listener_address: SocketAddr,
    pub metrics_listener_address: SocketAddr,
    pub internal_listener_address: SocketAddr,
    pub sentry: Option<SentryConfig>,
    pub authn: AuthnConfig,
    pub websocket: WebSocketConfig,
    pub authz: Authz,
    pub svc_audience: String,
    pub nats: Option<NatsConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WebSocketConfig {
    #[serde(with = "humantime_serde")]
    pub ping_interval: Duration,
    #[serde(with = "humantime_serde")]
    pub pong_expiration_interval: Duration,
    #[serde(with = "humantime_serde")]
    pub authentication_timeout: Duration,
    #[serde(with = "humantime_serde")]
    pub wait_before_close_connection: Duration,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NatsConfig {
    pub url: String,
}

pub fn load() -> Result<Config, config::ConfigError> {
    config::Config::builder()
        .add_source(config::File::with_name("App"))
        .add_source(config::Environment::with_prefix("APP").separator("__"))
        .build()
        .and_then(|c| c.try_deserialize::<Config>())
}
