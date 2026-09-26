//! `cargo web` — builds and serves the Watchgrid web UI using nothing but Cargo.
//!
//! build: compile `watchgrid-web` to wasm32, run wasm-bindgen (as a library, so
//!        its version is pinned by Cargo.lock), copy static files to dist/.
//! serve: build, serve dist/ over HTTP, rebuild on source changes and
//!        live-reload the browser.

mod build;
mod proxy;
mod server;
mod watch;

pub use build::build;

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo web <command> [options]

commands:
  build           build the web UI into dist/
  serve           build, serve on http://127.0.0.1:8080 and rebuild on change

options:
  --release       optimised build (smaller wasm)
  --live          use the real Watchgrid API (same origin; for installs)
  --port <N>      port for `serve` (default 8080)
  --host <ADDR>   bind address for `serve` (default 127.0.0.1)
  --api <URL>     use the real Watchgrid server for migrated API domains
                  (builds with `live-api`; `serve` proxies /api/* to URL),
                  e.g. --api http://127.0.0.1:8090";

#[derive(Clone)]
pub struct Opts {
    pub release: bool,
    pub port: u16,
    pub host: String,
    /// Backend base URL when running against the real server.
    pub api: Option<String>,
    /// Build for the real API without a dev proxy (production bundle).
    pub live: bool,
}

impl Opts {
    /// Talk to the real server (instead of the in-browser mock)?
    pub fn real_api(&self) -> bool {
        self.live || self.api.is_some()
    }
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };

    let mut opts = Opts { release: false, port: 8080, host: "127.0.0.1".into(), api: None, live: false };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--release" => opts.release = true,
            "--live" => opts.live = true,
            "--port" => match args.next().and_then(|p| p.parse().ok()) {
                Some(p) => opts.port = p,
                None => return fail("--port needs a number"),
            },
            "--host" => match args.next() {
                Some(h) => opts.host = h,
                None => return fail("--host needs an address"),
            },
            "--api" => match args.next().filter(|u| u.starts_with("http://")) {
                Some(u) => opts.api = Some(u.trim_end_matches('/').to_string()),
                None => return fail("--api needs an http:// URL"),
            },
            other => return fail(&format!("unknown option `{other}`\n\n{USAGE}")),
        }
    }

    let result = match cmd.as_str() {
        "build" => build(&opts).map(|_| ()),
        "serve" => server::serve(&opts),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        other => return fail(&format!("unknown command `{other}`\n\n{USAGE}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e),
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::FAILURE
}

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask lives in the workspace").to_path_buf()
}

pub fn web_dir() -> PathBuf {
    root().join("crates/web")
}

pub fn dist_dir() -> PathBuf {
    root().join("dist")
}
