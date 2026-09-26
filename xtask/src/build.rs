//! Build pipeline: cargo → wasm-bindgen → optional wasm-opt → static files.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs, io};

use crate::{Opts, dist_dir, root, web_dir};

const WEB_CRATE: &str = "watchgrid-web";
const WASM_TARGET: &str = "wasm32-unknown-unknown";

/// Full build. Returns the build id embedded in index.html.
pub fn build(opts: &Opts) -> Result<String, String> {
    let started = std::time::Instant::now();
    let profile = if opts.release { "release" } else { "debug" };
    let mode = if opts.real_api() { ", live API" } else { ", mock API" };
    println!("  building {WEB_CRATE} ({profile}, {WASM_TARGET}{mode})");

    // 1. Compile to wasm.
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.current_dir(root()).args(["build", "--package", WEB_CRATE, "--target", WASM_TARGET]);
    if opts.release {
        cmd.arg("--release");
    }
    if opts.real_api() {
        cmd.args(["--features", "live-api"]);
    }
    let status = cmd.status().map_err(|e| format!("failed to run cargo: {e}"))?;
    if !status.success() {
        return Err("cargo build failed".into());
    }

    let target_dir = env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| root().join("target"));
    let wasm = target_dir.join(WASM_TARGET).join(profile).join(format!("{}.wasm", WEB_CRATE.replace('-', "_")));

    // 2. Generate JS bindings into a staging dir, then swap it in so a
    //    running server never sees a half-written dist/.
    let dist = dist_dir();
    let staging = root().join("target/dist-staging");
    reset_dir(&staging).map_err(|e| format!("preparing {}: {e}", staging.display()))?;

    wasm_bindgen_cli_support::Bindgen::new()
        .input_path(&wasm)
        .web(true)
        .map_err(|e| e.to_string())?
        .typescript(false)
        .debug(!opts.release)
        .generate(&staging)
        .map_err(|e| format!("wasm-bindgen: {e}"))?;

    if opts.release {
        optimise_wasm(&staging.join(format!("{}_bg.wasm", WEB_CRATE.replace('-', "_"))));
    }

    // 3. Static files.
    let web = web_dir();
    let build_id = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0).to_string();
    let index = fs::read_to_string(web.join("index.html")).map_err(|e| format!("reading index.html: {e}"))?;
    fs::write(staging.join("index.html"), index.replace("{{BUILD}}", &build_id)).map_err(|e| e.to_string())?;
    bundle_css(&web.join("style"), &staging.join("style/app.css")).map_err(|e| format!("bundling style/: {e}"))?;
    copy_dir(&web.join("assets"), &staging.join("assets")).map_err(|e| format!("copying assets/: {e}"))?;

    let _ = fs::remove_dir_all(&dist);
    fs::rename(&staging, &dist).map_err(|e| format!("moving build to dist/: {e}"))?;

    let wasm_size = fs::metadata(dist.join(format!("{}_bg.wasm", WEB_CRATE.replace('-', "_")))).map(|m| m.len()).unwrap_or(0);
    println!("  finished in {:.1}s → dist/ (wasm {} KiB)", started.elapsed().as_secs_f32(), wasm_size / 1024);
    Ok(build_id)
}

/// Run binaryen's wasm-opt when it happens to be installed. Optional.
fn optimise_wasm(path: &Path) {
    let before = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    // Write to a temp file so a failed run never leaves a broken wasm behind.
    let tmp = path.with_extension("opt.wasm");
    // Enable every feature rustc emits for wasm32 by default; older binaryen
    // releases (e.g. distro packages) reject them otherwise.
    let result = Command::new("wasm-opt")
        .args([
            "-Oz",
            "--enable-bulk-memory",
            "--enable-nontrapping-float-to-int",
            "--enable-sign-ext",
            "--enable-mutable-globals",
            "--enable-reference-types",
            "--enable-multivalue",
        ])
        .arg(path)
        .arg("-o")
        .arg(&tmp)
        .output();

    match result {
        Err(_) => println!("  (wasm-opt not installed; skipping extra size optimisation — `apt install binaryen` to enable)"),
        Ok(out) if !out.status.success() => {
            let _ = fs::remove_file(&tmp);
            let err = String::from_utf8_lossy(&out.stderr);
            println!("  (wasm-opt failed, keeping unoptimised wasm: {})", err.lines().next().unwrap_or("unknown error"));
        }
        Ok(_) => {
            if fs::rename(&tmp, path).is_ok() {
                let after = fs::metadata(path).map(|m| m.len()).unwrap_or(before);
                println!("  wasm-opt: {} KiB → {} KiB", before / 1024, after / 1024);
            }
        }
    }
}

/// Join every `*.css` file in `dir`, in file-name order, into one stylesheet.
fn bundle_css(dir: &Path, out: &Path) -> io::Result<()> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "css"))
        .collect();
    files.sort();
    let mut css = String::new();
    for f in files {
        let name = f.file_name().unwrap_or_default().to_string_lossy();
        css.push_str(&format!("/* ==== {name} ==== */\n"));
        css.push_str(&fs::read_to_string(&f)?);
        css.push('\n');
    }
    fs::create_dir_all(out.parent().unwrap_or(dir))?;
    fs::write(out, css)
}

fn reset_dir(dir: &Path) -> io::Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}
