//! PostgreSQL pool and schema migrations (applied at startup).

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub async fn connect(url: &str) -> Result<PgPool, String> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(url)
        .await
        .map_err(|e| format!("cannot connect to PostgreSQL: {e}"))?;
    sqlx::migrate!("./migrations").run(&pool).await.map_err(|e| format!("database migration failed: {e}"))?;
    Ok(pool)
}
