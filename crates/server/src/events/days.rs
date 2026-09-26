//! Whole days in the NVR's time zone as instants: midnight to midnight,
//! worked out by PostgreSQL's zone database (DST days are 23 or 25 hours).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use watchgrid_model::NvrDays;

/// `[from, to)` for `days` in zone `tz`.
pub async fn bounds(db: &PgPool, tz: &str, days: NvrDays) -> sqlx::Result<(DateTime<Utc>, DateTime<Utc>)> {
    // First day as a local date, and how many days.
    let (first, count): (Option<chrono::NaiveDate>, i32) = match days {
        NvrDays::Today => (None, 1),
        NvrDays::Yesterday => (None, 1),
        NvrDays::Week => (None, 7),
        NvrDays::Date(d) => (Some(d), 1),
    };
    let back = match days {
        NvrDays::Yesterday => 1,
        NvrDays::Week => 6,
        _ => 0,
    };
    sqlx::query_as(
        "WITH d AS (SELECT COALESCE($2::date, (now() AT TIME ZONE $1)::date - $3::int) AS first)
         SELECT (d.first::timestamp AT TIME ZONE $1), ((d.first + $4::int)::timestamp AT TIME ZONE $1) FROM d",
    )
    .bind(tz)
    .bind(first)
    .bind(back)
    .bind(count)
    .fetch_one(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_local_day_is_midnight_to_midnight_in_the_zone(db: PgPool) {
        let day = |d: &str| NvrDays::Date(d.parse().unwrap());
        assert_eq!(bounds(&db, "Europe/Bucharest", day("2026-09-27")).await.unwrap(), (t("2026-09-26T21:00:00Z"), t("2026-09-27T21:00:00Z")));
        // DST ends on 25 October 2026: that day lasts 25 hours.
        assert_eq!(bounds(&db, "Europe/Bucharest", day("2026-10-25")).await.unwrap(), (t("2026-10-24T21:00:00Z"), t("2026-10-25T22:00:00Z")));
        assert_eq!(bounds(&db, "UTC", day("2026-09-27")).await.unwrap(), (t("2026-09-27T00:00:00Z"), t("2026-09-28T00:00:00Z")));

        let (from, to) = bounds(&db, "Europe/Bucharest", NvrDays::Today).await.unwrap();
        assert!(from <= Utc::now() && Utc::now() < to);
        let (week_from, week_to) = bounds(&db, "Europe/Bucharest", NvrDays::Week).await.unwrap();
        assert_eq!(week_to, to);
        assert!((from - week_from).num_hours() >= 6 * 24 - 1);
        let (y_from, y_to) = bounds(&db, "Europe/Bucharest", NvrDays::Yesterday).await.unwrap();
        assert_eq!(y_to, from);
        assert!(y_from < y_to);
    }
}
