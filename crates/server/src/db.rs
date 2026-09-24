//! PostgreSQL pool and schema migrations (applied at startup).

use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub async fn connect(url: &str) -> Result<PgPool, String> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect(url)
        .await
        .map_err(|e| format!("cannot connect to PostgreSQL at {}: {e}", describe(url)))?;
    sqlx::migrate!("./migrations").run(&pool).await.map_err(|e| format!("database migration failed: {e}"))?;
    Ok(pool)
}

/// `user@host:port/database` — never the password.
fn describe(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => format!("{}@{}:{}{}", u.username(), u.host_str().unwrap_or("?"), u.port().unwrap_or(5432), u.path()),
        Err(_) => "(DATABASE_URL is not a valid URL)".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::describe;

    #[test]
    fn describes_without_the_password() {
        let d = describe("postgres://watchgrid:s3cret@127.0.0.1:5433/watchgrid_nas");
        assert_eq!(d, "watchgrid@127.0.0.1:5433/watchgrid_nas");
        assert!(!d.contains("s3cret"));
        assert_eq!(describe("postgres://w:x@db/w"), "w@db:5432/w");
    }
}
