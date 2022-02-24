use sqlx::{postgres::PgPoolOptions, PgPool};
use std::env::var;
use std::time::Duration;

pub mod agent_session;

const DEFAULT_POOL_SIZE: u32 = 5;
const DEFAULT_POOL_IDLE_SIZE: u32 = 1;
const DEFAULT_POOL_TIMEOUT: u64 = 5;
const DEFAULT_POOL_MAX_LIFETIME: u64 = 1800;

pub async fn new_pool() -> PgPool {
    let url = var("DATABASE_URL").expect("DATABASE_URL must be specified");

    let size = var("DATABASE_POOL_SIZE")
        .map(|val| {
            val.parse::<u32>()
                .expect("Error converting DATABASE_POOL_SIZE variable into u32")
        })
        .unwrap_or(DEFAULT_POOL_SIZE);

    let idle_size = var("DATABASE_POOL_IDLE_SIZE")
        .map(|val| {
            val.parse::<u32>()
                .expect("Error converting DATABASE_POOL_IDLE_SIZE variable into u32")
        })
        .unwrap_or(DEFAULT_POOL_IDLE_SIZE);

    let timeout = var("DATABASE_POOL_TIMEOUT")
        .map(|val| {
            val.parse::<u64>()
                .expect("Error converting DATABASE_POOL_TIMEOUT variable into u64")
        })
        .unwrap_or(DEFAULT_POOL_TIMEOUT);

    let max_lifetime = var("DATABASE_POOL_MAX_LIFETIME")
        .map(|val| {
            val.parse::<u64>()
                .expect("Error converting DATABASE_POOL_MAX_LIFETIME variable into u64")
        })
        .unwrap_or(DEFAULT_POOL_MAX_LIFETIME);

    PgPoolOptions::new()
        .max_connections(size)
        .min_connections(idle_size)
        .connect_timeout(Duration::from_secs(timeout))
        .max_lifetime(Duration::from_secs(max_lifetime))
        .connect(&url)
        .await
        .expect("Failed to create sqlx database pool")
}
