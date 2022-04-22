use ::tracing::info;
use anyhow::Result;

mod app;
mod authz;
mod authz_hack;
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
    app::run(db, authz_cache).await
}
