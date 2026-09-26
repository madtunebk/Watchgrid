//! `watchgrid storage …` — show or change the recordings folder.
//!
//! As root, `set-path` also creates the folder, gives it to the
//! `watchgrid` user and opens it in the systemd sandbox (the web UI can't:
//! it runs unprivileged and may only pick folders it can already write).

use std::path::Path;
use std::process::Command;

use sqlx::PgPool;

use super::{disk, location};
use crate::config::Config;

pub const USAGE: &str = "\
usage: watchgrid storage <command>

commands:
  show                 current recordings folder, free space and clip count
  set-path <folder>    record into <folder> from now on (as root: create it,
                       give it to the watchgrid user, allow it in systemd)";

const SERVICE_USER: &str = "watchgrid";
const UNIT: &str = "/etc/systemd/system/watchgrid.service";
const DROP_IN_DIR: &str = "/etc/systemd/system/watchgrid.service.d";
const DROP_IN: &str = "/etc/systemd/system/watchgrid.service.d/recordings.conf";

pub async fn run(db: &PgPool, config: &Config, args: &[String]) -> Result<(), String> {
    match (args.first().map(String::as_str), args.get(1)) {
        (Some("show"), _) => show(db, config).await,
        (Some("set-path"), Some(path)) => set_path(db, path).await,
        _ => Err(USAGE.into()),
    }
}

async fn show(db: &PgPool, config: &Config) -> Result<(), String> {
    let chosen = location::load(db).await.map_err(|e| e.to_string())?;
    let current = chosen.clone().unwrap_or_else(|| config.recordings_dir.clone());
    println!("recordings folder: {}{}", current.display(), if chosen.is_some() { "  (chosen in Settings/CLI)" } else { "  (default)" });
    match disk::space(&current) {
        Ok(d) => println!("free: {:.1} GB of {:.1} GB", d.free as f64 / 1e9, d.total as f64 / 1e9),
        Err(e) => println!("free: unknown ({e})"),
    }
    let (count, bytes): (i64, Option<i64>) = sqlx::query_as("SELECT COUNT(*), SUM(file_size)::bigint FROM recordings").fetch_one(db).await.map_err(|e| e.to_string())?;
    println!("recordings: {count} ({:.2} GB, in all folders)", bytes.unwrap_or(0) as f64 / 1e9);
    Ok(())
}

async fn set_path(db: &PgPool, path: &str) -> Result<(), String> {
    let p = location::check_shape(path)?;
    if is_root() {
        std::fs::create_dir_all(&p).map_err(|e| format!("cannot create {}: {e}", p.display()))?;
        if let Some((uid, gid)) = service_user() {
            chown(&p, uid, gid)?;
            println!("{} belongs to the {SERVICE_USER} user", p.display());
        }
        if Path::new(UNIT).exists() {
            allow_in_sandbox(&p)?;
        }
    }
    location::check_writable(&p.to_string_lossy())?;
    location::save(db, &p).await.map_err(|e| e.to_string())?;
    println!("New recordings will be written to {}. Existing clips stay where they are.", p.display());
    if is_root() && Path::new(UNIT).exists() {
        systemctl(&["daemon-reload"])?;
        systemctl(&["try-restart", "watchgrid"])?;
        println!("watchgrid restarted (running recordings were finalized first).");
    } else {
        println!("A running server picks this up on its next restart; or change it in Settings → Storage.");
    }
    Ok(())
}

fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

fn service_user() -> Option<(u32, u32)> {
    let name = std::ffi::CString::new(SERVICE_USER).ok()?;
    // SAFETY: valid C string; the returned record is read immediately.
    let pw = unsafe { libc::getpwnam(name.as_ptr()) };
    if pw.is_null() {
        return None;
    }
    // SAFETY: non-null pointer from getpwnam.
    Some(unsafe { ((*pw).pw_uid, (*pw).pw_gid) })
}

fn chown(path: &Path, uid: u32, gid: u32) -> Result<(), String> {
    std::os::unix::fs::chown(path, Some(uid), Some(gid)).map_err(|e| format!("cannot give {} to {SERVICE_USER}: {e}", path.display()))
}

/// Add the folder to the service's writable paths (ProtectSystem=strict).
fn allow_in_sandbox(p: &Path) -> Result<(), String> {
    let line = format!("ReadWritePaths={}", p.display());
    let existing = std::fs::read_to_string(DROP_IN).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == line) {
        return Ok(());
    }
    let mut content = if existing.trim().is_empty() {
        "# Recording folders chosen with `watchgrid storage set-path`.\n[Service]\n".to_string()
    } else {
        existing
    };
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&line);
    content.push('\n');
    std::fs::create_dir_all(DROP_IN_DIR).map_err(|e| e.to_string())?;
    std::fs::write(DROP_IN, content).map_err(|e| format!("cannot write {DROP_IN}: {e}"))?;
    println!("allowed {} in {DROP_IN}", p.display());
    Ok(())
}

fn systemctl(args: &[&str]) -> Result<(), String> {
    let status = Command::new("systemctl").args(args).status().map_err(|e| format!("systemctl: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("systemctl {} failed", args.join(" "))) }
}
