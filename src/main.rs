use ::tracing::info;
use anyhow::Result;

mod app;
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
    app::run(db).await
}

#[cfg(test)]
mod test {
    use crate::test_helpers::test_container::TestContainer;

    #[test]
    fn test_containers_works() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();

        dbg!(postgres.connection_string);
        assert!(true);
    }
}
