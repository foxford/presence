use sqlx::{pool::PoolConnection, postgres::PgPoolOptions, PgPool, Postgres};

#[derive(Clone)]
pub struct TestDb {
    pool: PgPool,
}

impl TestDb {
    pub async fn new(url: &str) -> Self {
        let pool = PgPoolOptions::new()
            .min_connections(1)
            .max_connections(1)
            .connect(&url)
            .await
            .expect("Failed to connect to the DB");

        let mut conn = pool.acquire().await.expect("Failed to get DB connection");

        sqlx::migrate!()
            .run(&mut conn)
            .await
            .expect("Failed to run migration");

        Self { pool }
    }

    pub async fn get_conn(&self) -> PoolConnection<Postgres> {
        self.pool
            .acquire()
            .await
            .expect("Failed to get DB connection")
    }
}
