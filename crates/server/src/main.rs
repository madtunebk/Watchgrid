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
mod bus;
mod cameras;
mod config;
mod credentials;
mod db;
mod error;
mod events;
mod http;
mod httpc;
mod live;
mod media;
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

const USAGE: &str = "usage:\n  watchgrid [serve]\n  watchgrid init\n  watchgrid storage show|set-path <folder>\n  watchgrid probe <camera-id> [--sub] [--seconds N]\n  watchgrid probe-onvif <camera-id> [--url URL]\n  watchgrid watch-onvif <camera-id> [--seconds N]\n  watchgrid user create|list|passwd|enable|disable|delete <username>\n\nEnvironment (or .env, or /etc/watchgrid/watchgrid.env): DATABASE_URL, WATCHGRID_BIND, WATCHGRID_DATA_DIR, WATCHGRID_KEY_FILE, WATCHGRID_RECORDINGS_DIR, WATCHGRID_UI_DIR";

#[tokio::main]
async fn main() -> ExitCode {
    config::load_env_files();
    let logs = system::logs::LogBuffer::default();
    {
        use tracing_subscriber::prelude::*;
        let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "watchgrid=info".into());
        tracing_subscriber::registry().with(filter).with(tracing_subscriber::fmt::layer()).with(system::logs::CaptureLayer(logs.clone())).init();
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
        Some("watch-onvif") => match args.get(1) {
            Some(id) => watch_onvif(id, args.iter().position(|a| a == "--seconds").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(30)).await,
            None => Err(format!("watch-onvif needs a camera id\n\n{USAGE}")),
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
    let mut state = open_state(&config).await?;
    state.logs = logs;
    state.bind = config.bind;
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
        state.auto_record.apply(&id, true);
    }
    tokio::spawn(state.retention.clone().run());
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
