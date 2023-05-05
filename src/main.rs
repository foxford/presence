use ::tracing::{error, info};
use anyhow::Result;
use std::process;

mod app;
mod authz;
mod classroom;
mod config;
mod db;
mod session;
#[cfg(test)]
mod test_helpers;
mod tracing;

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(feature = "dotenv")]
    dotenv::dotenv()?;

    let _guard = tracing::init()?;

    info!(
        "Launching {}, version: {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let db = db::new_pool().await;
    let authz_cache = authz::new_cache();

    if let Err(e) = app::run(db, authz_cache).await {
        error!(error = %e, "Failed to launch presence");
        process::exit(1);
    }

    Ok(())
}
