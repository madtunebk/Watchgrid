//! `watchgrid` — the Watchgrid NVR server.
//!
//! Usage:
//!   watchgrid [serve]                     run the server (default)
//!   watchgrid probe <camera-id> [--sub] [--seconds N]
//!                                         connect to a camera's stream and report what it sends
//!   watchgrid --help
//!
//! User accounts are managed from this CLI in a later milestone
//! (`watchgrid user …`); the web UI never creates or resets users.

mod bus;
mod cameras;
mod config;
mod credentials;
mod db;
mod error;
mod events;
mod http;
mod live;
mod media;
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

const USAGE: &str = "usage:\n  watchgrid [serve]\n  watchgrid probe <camera-id> [--sub] [--seconds N]\n\nEnvironment (or .env): DATABASE_URL, WATCHGRID_BIND, WATCHGRID_DATA_DIR, WATCHGRID_UI_DIR";

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();
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

async fn open_state(config: &Config) -> Result<AppState, String> {
    let db = db::connect(&config.database_url).await?;
    let credentials = CredentialStore::load_or_create(&config.data_dir).map_err(|e| format!("credential store: {e}"))?;
    Ok(AppState::new(db, credentials, config.data_dir.join("recordings")))
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
    state.recording_files.prepare().map_err(|e| format!("recordings directory: {e}"))?;
    // Before the supervisor starts, so no transition is missed.
    events::start_journal(state.db.clone(), state.bus.clone());
    // Start supervising every configured camera.
    for (id, enabled) in cameras::all_ids(&state).await.map_err(|_| "cannot list cameras".to_string())? {
        state.supervisor.apply(&id, enabled);
    }
    tokio::spawn(state.retention.clone().run());
    let recorder = state.recorder.clone();
    let app = http::router(state, &config.ui_dir);

    let listener = tokio::net::TcpListener::bind(config.bind).await.map_err(|e| format!("cannot listen on {}: {e}", config.bind))?;
    tracing::info!("Watchgrid listening on http://{}", config.bind);
    axum::serve(listener, app)
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
