use ::tracing::info;
use anyhow::Result;

mod app;
mod authz;
mod classroom;
mod config;
mod db;
mod state;
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
    app::run(db, authz_cache).await
}
