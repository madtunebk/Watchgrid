//! Settings document persistence and validation (`sqlx::test`).

use sqlx::PgPool;

use super::app;

fn bind() -> std::net::SocketAddr {
    "127.0.0.1:8090".parse().unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn defaults_then_round_trip(db: PgPool) {
    let mut s = app::load(&db, bind()).await.unwrap();
    assert_eq!((s.general.nvr_name.as_str(), s.network.http_port), ("Watchgrid", 8090));
    s.general.nvr_name = "  Home NVR ".into();
    s.general.timezone = "Europe/Bucharest".into();
    s.notifications.webhook_url = Some("  ".into());
    let saved = app::save(&db, s).await.unwrap();
    assert_eq!(saved.general.nvr_name, "Home NVR", "trimmed");
    assert_eq!(saved.notifications.webhook_url, None, "blank webhook cleared");
    assert_eq!(app::load(&db, bind()).await.unwrap(), saved);
}

#[sqlx::test(migrations = "./migrations")]
async fn invalid_settings_are_refused(db: PgPool) {
    let base = app::defaults(bind());
    let cases: Vec<Box<dyn Fn(&mut watchgrid_model::Settings)>> = vec![
        Box::new(|s| s.general.nvr_name = " ".into()),
        Box::new(|s| s.general.timezone = "Mars/Olympus_Mons".into()),
        Box::new(|s| s.network.http_port = 0),
        Box::new(|s| s.network.http_bind = "localhost".into()),
        Box::new(|s| s.advanced.reconnect_seconds = 0),
        Box::new(|s| s.notifications.webhook_url = Some("ftp://x".into())),
    ];
    for (i, change) in cases.iter().enumerate() {
        let mut s = base.clone();
        change(&mut s);
        assert!(app::save(&db, s).await.is_err(), "case {i} should be refused");
    }
    assert_eq!(app::load(&db, bind()).await.unwrap(), base, "nothing was stored");
}

#[sqlx::test(migrations = "./migrations")]
async fn local_clock_follows_the_configured_zone(db: PgPool) {
    let mut s = app::defaults(bind());
    s.general.timezone = "Pacific/Kiritimati".into(); // UTC+14
    app::save(&db, s).await.unwrap();
    let (day, minute) = super::local_clock(&db).await.unwrap();
    let utc: (i32, i32) = sqlx::query_as("SELECT EXTRACT(ISODOW FROM now() AT TIME ZONE 'UTC')::int - 1, (EXTRACT(HOUR FROM now() AT TIME ZONE 'UTC') * 60 + EXTRACT(MINUTE FROM now() AT TIME ZONE 'UTC'))::int")
        .fetch_one(&db)
        .await
        .unwrap();
    let local = i32::from(day) * 1440 + i32::from(minute);
    let expected = (utc.0 * 1440 + utc.1 + 14 * 60).rem_euclid(7 * 1440);
    assert!((local - expected).abs() <= 1, "14 hours ahead of UTC");
}
