use serde_derive::Deserialize;
use svc_authn::jose::ConfigMap as AuthnConfig;
use svc_error::extension::sentry::Config as SentryConfig;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Config {
    pub(crate) listener_address: String,
    pub(crate) sentry: Option<SentryConfig>,
    pub(crate) authn: AuthnConfig,
    pub(crate) websocket: WebSocketConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct WebSocketConfig {
    pub(crate) ping_interval: u64,
    pub(crate) pong_expiration_interval: u64,
    pub(crate) authentication_timeout: u64,
}

pub(crate) fn load() -> Result<Config, config::ConfigError> {
    let mut parser = config::Config::default();
    parser.merge(config::File::with_name("presence"))?;
    parser.merge(config::Environment::with_prefix("APP").separator("__"))?;
    parser.try_into::<Config>()
}
