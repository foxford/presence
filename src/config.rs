use serde_derive::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Config {
    pub(crate) listener_address: String,
}

pub(crate) fn load() -> Result<Config, config::ConfigError> {
    let mut parser = config::Config::default();
    parser.merge(config::File::with_name("App"))?;
    parser.merge(config::Environment::with_prefix("APP").separator("__"))?;
    parser.try_into::<Config>()
}
