use serde_derive::Deserialize;
use std::net::SocketAddr;
use std::time::Duration;
use svc_authn::jose::ConfigMap as AuthnConfig;
use svc_error::extension::sentry::Config as SentryConfig;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Config {
    pub(crate) listener_address: SocketAddr,
    pub(crate) sentry: Option<SentryConfig>,
    pub(crate) authn: AuthnConfig,
    pub(crate) websocket: WebSocketConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct WebSocketConfig {
    #[serde(with = "humantime_serde")]
    pub(crate) ping_interval: Duration,
    #[serde(with = "humantime_serde")]
    pub(crate) pong_expiration_interval: Duration,
    #[serde(with = "humantime_serde")]
    pub(crate) authentication_timeout: Duration,
}

pub(crate) fn load() -> Result<Config, config::ConfigError> {
    let mut parser = config::Config::default();
    parser.merge(config::File::with_name("presence"))?;
    parser.merge(config::Environment::with_prefix("APP").separator("__"))?;
    parser.try_into::<Config>()
}
