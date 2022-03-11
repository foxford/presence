use serde_derive::Deserialize;
use std::{net::SocketAddr, time::Duration};
use svc_authn::jose::ConfigMap as AuthnConfig;
use svc_authn::AccountId;
use svc_authz::ConfigMap as Authz;
use svc_error::extension::sentry::Config as SentryConfig;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub id: AccountId,
    pub listener_address: SocketAddr,
    pub sentry: Option<SentryConfig>,
    pub authn: AuthnConfig,
    pub websocket: WebSocketConfig,
    pub authz: Authz,
    pub svc_audience: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WebSocketConfig {
    #[serde(with = "humantime_serde")]
    pub ping_interval: Duration,
    #[serde(with = "humantime_serde")]
    pub pong_expiration_interval: Duration,
    #[serde(with = "humantime_serde")]
    pub authentication_timeout: Duration,
}

pub fn load() -> Result<Config, config::ConfigError> {
    let mut parser = config::Config::default();
    parser.merge(config::File::with_name("presence"))?;
    parser.merge(config::Environment::with_prefix("APP").separator("__"))?;
    parser.try_into::<Config>()
}
