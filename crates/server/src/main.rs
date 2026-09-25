//! `watchgrid` — the Watchgrid NVR server.
//!
//! Usage:
//!   watchgrid [serve]                     run the server (default)
//!   watchgrid probe <camera-id> [--sub] [--seconds N]
//!                                         connect to a camera's stream and report what it sends
//!   watchgrid --help
//!
//!   watchgrid user create|list|passwd|enable|disable|delete <username>
//!
//! User accounts exist only through this CLI; the web UI never creates,
//! deletes or resets users.

mod auth;
mod backup;
mod bus;
mod cameras;
mod capacity;
mod config;
mod credentials;
mod db;
mod error;
mod events;
mod exports;
mod http;
mod httpc;
mod live;
mod media;
mod motion;
mod notifications;
mod onvif;
mod recorder;
mod recordings;
mod rtsp;
mod settings;
mod state;
mod storage;
mod supervisor;
mod system;
mod timezone;
mod ws;

use std::process::ExitCode;

use config::Config;
use credentials::CredentialStore;
use state::AppState;

const USAGE: &str = "usage:\n  watchgrid [serve]\n  watchgrid init\n  watchgrid storage show|set-path <folder>\n  watchgrid probe <camera-id> [--sub] [--seconds N]\n  watchgrid live-dump <camera-id> [--sub] [--seconds N] [--out FILE]\n  watchgrid probe-onvif <camera-id> [--url URL]\n  watchgrid watch-onvif <camera-id> [--seconds N]\n  watchgrid user create|list|passwd|enable|disable|delete <username>\n  watchgrid backup [--out FILE]\n  watchgrid restore <FILE> [--replace]\n\nEnvironment (or .env, or /etc/watchgrid/watchgrid.env): DATABASE_URL, WATCHGRID_BIND, WATCHGRID_DATA_DIR, WATCHGRID_KEY_FILE, WATCHGRID_RECORDINGS_DIR, WATCHGRID_UI_DIR, WATCHGRID_BACKUP_DIR";

#[tokio::main]
async fn main() -> ExitCode {
    config::load_env_files();
    let logs = system::logs::LogBuffer::default();
    {
        use tracing_subscriber::prelude::*;
        // Settings → Advanced → Log level can change it later, unless RUST_LOG is set.
        let (filter, managed) = settings::applied::initial_log_filter();
        let (filter, handle) = tracing_subscriber::reload::Layer::new(filter);
        if managed {
            settings::applied::manage_log_level(handle);
        }
        // Colours only on a terminal (not in journald or `docker logs`).
        let ansi = std::io::IsTerminal::is_terminal(&std::io::stdout());
        tracing_subscriber::registry().with(filter).with(tracing_subscriber::fmt::layer().with_ansi(ansi)).with(system::logs::CaptureLayer(logs.clone())).init();
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        None | Some("serve") => serve(logs).await,
        Some("probe") => match args.get(1) {
            Some(id) => {
                let seconds = args.iter().position(|a| a == "--seconds").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(5);
                probe(id, args.iter().any(|a| a == "--sub"), seconds).await
            }
            None => Err(format!("probe needs a camera id\n\n{USAGE}")),
        },
        Some("user") => user(&args[1..]).await,
        Some("storage") => storage_cmd(&args[1..]).await,
        Some("init") => init().await,
        Some("backup") => backup_cmd(args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from)).await,
        Some("restore") => match args.get(1).filter(|a| !a.starts_with("--")) {
            Some(file) => restore_cmd(std::path::Path::new(file), args.iter().any(|a| a == "--replace")).await,
            None => Err(format!("restore needs a backup file\n\n{USAGE}")),
        },
        Some("watch-onvif") => match args.get(1) {
            Some(id) => watch_onvif(id, args.iter().position(|a| a == "--seconds").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(30)).await,
            None => Err(format!("watch-onvif needs a camera id\n\n{USAGE}")),
        },
        Some("live-dump") => match args.get(1) {
            Some(id) => {
                let seconds = args.iter().position(|a| a == "--seconds").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(10);
                let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| format!("{id}-live.mp4"));
                live_dump(id, args.iter().any(|a| a == "--sub"), seconds, &out).await
            }
            None => Err(format!("live-dump needs a camera id\n\n{USAGE}")),
        },
        Some("probe-onvif") => match args.get(1) {
            Some(id) => probe_onvif(id, args.iter().position(|a| a == "--url").and_then(|i| args.get(i + 1)).map(String::as_str)).await,
            None => Err(format!("probe-onvif needs a camera id\n\n{USAGE}")),
        },
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `watchgrid backup`: database + master key into one file.
async fn backup_cmd(out: Option<std::path::PathBuf>) -> Result<(), String> {
    let config = Config::from_env()?;
    let db = db::connect(&config.database_url).await?;
    let out = out.unwrap_or_else(|| backup::default_path(&config.backup_dir));
    let m = backup::create(&db, &config.key_file, &out).await?;
    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    println!("backup written: {} ({:.1} MB, {} tables, schema {})", out.display(), size as f64 / 1e6, m.tables.len(), m.schema_version);
    println!("It contains the master key: keep it as private as the key itself. Recordings are not included.");
    Ok(())
}

/// `watchgrid restore`: the server must be stopped.
async fn restore_cmd(file: &std::path::Path, replace: bool) -> Result<(), String> {
    let config = Config::from_env()?;
    let options: sqlx::postgres::PgConnectOptions = config.database_url.parse().map_err(|e| format!("DATABASE_URL: {e}"))?;
    let r = backup::restore(&options, &config.key_file, file, replace).await?;
    println!("restored the backup of {} (build {}, {} tables)", r.manifest.created_at.format("%Y-%m-%d %H:%M UTC"), r.manifest.build, r.manifest.tables.len());
    if let Some(old) = r.old_key {
        println!("the previous master key was kept as {}", old.display());
    }
    println!("Start Watchgrid again. Recordings are not part of backups: their files must still be in the recordings folder.");
    Ok(())
}

/// Diagnostics: save the live stream as the browser gets it.
async fn live_dump(id: &str, sub: bool, seconds: u64, out: &str) -> Result<(), String> {
    let config = Config::from_env()?;
    let state = open_state(&config).await?;
    let kind = if sub { media::StreamKind::Sub } else { media::StreamKind::Main };
    media::dump::run(&state.media, id, kind, seconds, out).await
}

/// Diagnostics: ONVIF device info and event topics with the stored login.
async fn probe_onvif(id: &str, url_override: Option<&str>) -> Result<(), String> {
    let config = Config::from_env()?;
    let state = open_state(&config).await?;
    let camera = cameras::get_camera(&state, id).await.map_err(|_| format!("no camera with id `{id}`"))?;
    let onvif = camera.onvif.ok_or("this camera has no ONVIF settings")?;
    let (user, password) = cameras::stored_onvif_login(&state, &onvif.url).await.map_err(|_| "cannot read the ONVIF credentials".to_string())?.unwrap_or_default();
    let url = url_override.unwrap_or(&onvif.url);
    println!("{} — {url} (user {})", camera.name, if user.is_empty() { "(none)" } else { &user });
    let p = onvif::probe(url, &user, password).await;
    println!("  {}: {}", if p.ok { "ok" } else { "FAILED" }, p.message);
    for t in &p.event_topics {
        println!("  topic: {t}");
    }
    println!("  detections: {:?}", p.detections);
    Ok(())
}

/// Diagnostics: print a camera's ONVIF events as they arrive.
async fn watch_onvif(id: &str, seconds: u64) -> Result<(), String> {
    let config = Config::from_env()?;
    let state = open_state(&config).await?;
    let camera = cameras::get_camera(&state, id).await.map_err(|_| format!("no camera with id `{id}`"))?;
    let onvif = camera.onvif.ok_or("this camera has no ONVIF settings")?;
    let (user, password) = cameras::stored_onvif_login(&state, &onvif.url).await.map_err(|_| "cannot read the ONVIF credentials".to_string())?.unwrap_or_default();
    let sub = onvif::pullpoint::Subscription::create(&onvif.url, &user, password).await?;
    println!("{} — subscribed; watching {seconds} s", camera.name);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while std::time::Instant::now() < until {
        for n in sub.pull().await? {
            println!("  {} {} {:?} {:?} {:?}", onvif::pullpoint::when(&n).format("%H:%M:%S"), n.operation, n.topic, n.active(), n.data);
        }
    }
    sub.unsubscribe().await;
    Ok(())
}

/// `watchgrid storage …`
async fn storage_cmd(args: &[String]) -> Result<(), String> {
    let config = Config::from_env()?;
    let db = db::connect(&config.database_url).await?;
    storage::cli::run(&db, &config, args).await
}

/// First-time setup (the installer runs it as root): create the credential
/// key if missing and bring the database schema up to date.
async fn init() -> Result<(), String> {
    let config = Config::from_env()?;
    CredentialStore::load_or_create(&config.key_file).map_err(|e| format!("credential key {}: {e}", config.key_file.display()))?;
    println!("credential key: {}", config.key_file.display());
    db::connect(&config.database_url).await?;
    println!("database: schema up to date");
    println!("recordings folder (default): {}", config.recordings_dir.display());
    Ok(())
}

/// `watchgrid user …`: needs only the database.
async fn user(args: &[String]) -> Result<(), String> {
    let config = Config::from_env()?;
    let db = db::connect(&config.database_url).await?;
    auth::cli::run(&db, args).await
}

async fn open_state(config: &Config) -> Result<AppState, String> {
    let db = db::connect(&config.database_url).await?;
    let credentials = CredentialStore::load_or_create(&config.key_file).map_err(|e| format!("credential store: {e}"))?;
    Ok(AppState::new(db, credentials, config.recordings_dir.clone()))
}

/// Diagnostics: connect to a configured camera and report its stream.
/// Prints no secrets.
async fn probe(id: &str, sub: bool, seconds: u64) -> Result<(), String> {
    let config = Config::from_env()?;
    let state = open_state(&config).await?;
    let camera = cameras::get_camera(&state, id).await.map_err(|_| format!("no camera with id `{id}`"))?;
    let (user, password) = cameras::stream_credentials(&state, id).await.map_err(|_| "cannot read the camera's credentials".to_string())?;
    let url = if sub {
        camera.sub_stream.map(|s| s.url).ok_or("this camera has no substream")?
    } else {
        camera.main_stream.url
    };
    println!("{} — {}", camera.name, url);
    println!("  user: {}  password: {}", if user.is_empty() { "(none)" } else { &user }, if password.is_some() { "(stored, encrypted)" } else { "(none)" });
    let r = rtsp::probe::probe(&url, &user, password.as_deref(), std::time::Duration::from_secs(seconds)).await?;
    println!("  connected in {} ms", r.connect_latency.as_millis());
    let f = &r.facts;
    println!("  video: {} {}", f.video_codec.as_deref().unwrap_or("?"), match (f.width, f.height) { (Some(w), Some(h)) => format!("{w}x{h}"), _ => "resolution unknown".into() });
    println!("  audio: {}", f.audio_codec.as_deref().unwrap_or("none"));
    println!("  measured {:.1} s: {} frames ({:.1} fps), {} keyframes, {:.0} kbit/s", r.measured.as_secs_f64(), r.frames, r.fps(), r.keyframes, r.kbps());
    Ok(())
}

async fn serve(logs: system::logs::LogBuffer) -> Result<(), String> {
    let config = Config::from_env()?;
    // One server per database; a restore waits for it to stop.
    let options: sqlx::postgres::PgConnectOptions = config.database_url.parse().map_err(|e| format!("DATABASE_URL: {e}"))?;
    let _server_lock = backup::take_server_lock(&options)
        .await?
        .ok_or("another Watchgrid (or a restore) is using this database; stop it first")?;
    let mut state = open_state(&config).await?;
    state.logs = logs;
    state.bind = config.bind;
    // Before any camera connects: transport, reconnect delay, log level.
    match settings::load_app(&state.db, state.bind).await {
        Ok(s) => {
            settings::applied::apply(&s.advanced);
        }
        Err(e) => tracing::warn!("cannot read the settings, using defaults: {}", e.message()),
    }
    tokio::spawn(state.metrics.clone().run());
    // A folder chosen in Settings/CLI wins over the configured default.
    match storage::location::load(&state.db).await {
        Ok(Some(p)) => match storage::location::check_writable(&p.to_string_lossy()) {
            Ok(p) => state.recording_files.set_root(p),
            Err(e) => tracing::error!("recordings folder {} is unusable, using {}: {e}", p.display(), config.recordings_dir.display()),
        },
        Ok(None) => {}
        Err(e) => tracing::warn!("cannot read the recordings folder setting: {e}"),
    }
    state.recording_files.prepare().map_err(|e| format!("recordings directory: {e}"))?;
    // Before the supervisor starts, so no transition is missed.
    events::start_journal(state.db.clone(), state.bus.clone());
    notifications::start(state.db.clone(), state.bus.clone(), config.bind, state.recording_files.clone());
    // Start supervising every configured camera.
    for (id, enabled) in cameras::all_ids(&state).await.map_err(|_| "cannot list cameras".to_string())? {
        state.supervisor.apply(&id, enabled);
        state.onvif.apply(&id, true);
        state.motion.apply(&id, true);
        state.auto_record.apply(&id, true);
    }
    tokio::spawn(state.retention.clone().run());
    backup::start_daily(state.db.clone(), config.key_file.clone(), config.backup_dir.clone());
    state.exports.start().await;
    exports_auto_upload(&state);
    let recorder = state.recorder.clone();
    let app = http::router(state, &config.ui_dir);

    let listener = tokio::net::TcpListener::bind(config.bind).await.map_err(|e| format!("cannot listen on {}: {e}", config.bind))?;
    tracing::info!("Watchgrid listening on http://{}", config.bind);
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("shutting down");
            // Never leave half-written recordings behind on a clean stop.
            recorder.shutdown().await;
        })
        .await
        .map_err(|e| e.to_string())
}

/// Auto-upload finished clips to destinations whose rule asks for it.
fn exports_auto_upload(state: &AppState) {
    let mut events = state.bus.subscribe();
    let exports = state.exports.clone();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(bus::BusEvent::RecordingStopped { recording_id: Some(id), .. }) => {
                    let exports = exports.clone();
                    tokio::spawn(async move {
                        // Let the event journal link the clip's events first.
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        exports.on_clip_saved(&id).await;
                    });
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => return,
            }
        }
    });
}

/// Ctrl+C, or SIGTERM from a service manager (systemd, Docker).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
