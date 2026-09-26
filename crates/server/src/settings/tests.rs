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
    let saved = app::save(&db, bind(), s).await.unwrap();
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
        Box::new(|s| s.advanced.reconnect_seconds = 0),
        Box::new(|s| s.notifications.webhook_url = Some("ftp://x".into())),
    ];
    for (i, change) in cases.iter().enumerate() {
        let mut s = base.clone();
        change(&mut s);
        assert!(app::save(&db, bind(), s).await.is_err(), "case {i} should be refused");
    }
    assert_eq!(app::load(&db, bind()).await.unwrap(), base, "nothing was stored");
}

#[sqlx::test(migrations = "./migrations")]
async fn local_clock_follows_the_configured_zone(db: PgPool) {
    let mut s = app::defaults(bind());
    s.general.timezone = "Pacific/Kiritimati".into(); // UTC+14
    app::save(&db, bind(), s).await.unwrap();
    let (day, minute) = super::local_clock(&db).await.unwrap();
    let utc: (i32, i32) = sqlx::query_as("SELECT EXTRACT(ISODOW FROM now() AT TIME ZONE 'UTC')::int - 1, (EXTRACT(HOUR FROM now() AT TIME ZONE 'UTC') * 60 + EXTRACT(MINUTE FROM now() AT TIME ZONE 'UTC'))::int")
        .fetch_one(&db)
        .await
        .unwrap();
    let local = i32::from(day) * 1440 + i32::from(minute);
    let expected = (utc.0 * 1440 + utc.1 + 14 * 60).rem_euclid(7 * 1440);
    assert!((local - expected).abs() <= 1, "14 hours ahead of UTC");
}

#[sqlx::test(migrations = "./migrations")]
async fn older_documents_load_and_the_network_is_read_only(db: PgPool) {
    // As saved by versions that still had language / auth.enabled / https.
    let mut old = serde_json::to_value(app::defaults(bind())).unwrap();
    old["general"]["language"] = "ro".into();
    old["auth"]["enabled"] = true.into();
    old["network"]["httpsEnabled"] = true.into();
    old["network"]["httpsPort"] = 8443.into();
    old["general"]["nvrName"] = "Old NVR".into();
    super::store::save(&db, "app", &old).await.unwrap();
    let mut s = app::load(&db, bind()).await.unwrap();
    assert_eq!(s.general.nvr_name, "Old NVR", "an older document still loads");

    s.network.http_port = 1;
    s.network.http_bind = "not an address".into();
    let saved = app::save(&db, bind(), s).await.unwrap();
    assert_eq!((saved.network.http_bind.as_str(), saved.network.http_port), ("127.0.0.1", 8090), "what was sent is replaced");
    let stored: serde_json::Value = super::store::load(&db, "app").await.unwrap().unwrap();
    assert!(stored["general"].get("language").is_none(), "old fields are gone once saved");
}
