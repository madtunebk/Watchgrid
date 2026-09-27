//! How fast recordings are written lately: the projection's input. A
//! recent window, not "everything kept ÷ age of the oldest clip", which
//! old protected clips, retention and new cameras all distort.

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use watchgrid_model::WriteRate;

/// At most this far back.
const WINDOW: Duration = Duration::days(7);
/// Less than this is too little to call a rate.
const MIN_WINDOW: Duration = Duration::hours(1);

/// Bytes per day from `bytes` written between `from` and `now`.
fn rate(bytes: u64, from: DateTime<Utc>, now: DateTime<Utc>) -> Option<WriteRate> {
    let window = now - from;
    if window < MIN_WINDOW {
        return None;
    }
    let hours = window.num_seconds() as f64 / 3600.0;
    Some(WriteRate { bytes_per_day: (bytes as f64 / hours * 24.0) as u64, window_hours: hours.round() as u32 })
}

pub async fn measure(db: &PgPool) -> sqlx::Result<Option<WriteRate>> {
    let now = Utc::now();
    let first: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT MIN(start_time) FROM recordings").fetch_one(db).await?;
    let Some(first) = first else { return Ok(None) };
    let from = first.max(now - WINDOW);
    let bytes: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(file_size), 0)::bigint FROM recordings WHERE start_time >= $1").bind(from).fetch_one(db).await?;
    Ok(rate(bytes.max(0) as u64, from, now))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_needs_an_hour_and_scales_to_a_day() {
        let now: DateTime<Utc> = "2026-09-27T12:00:00Z".parse().unwrap();
        assert_eq!(rate(1_000, now - Duration::minutes(30), now), None, "too early to tell");
        assert_eq!(rate(1_000_000, now - Duration::hours(12), now), Some(WriteRate { bytes_per_day: 2_000_000, window_hours: 12 }));
        assert_eq!(rate(7_000_000, now - Duration::days(7), now), Some(WriteRate { bytes_per_day: 1_000_000, window_hours: 168 }));
    }
}
