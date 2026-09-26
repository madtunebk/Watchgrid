# Watchgrid

Self-hosted NVR written in Rust. Watch, record and review your IP cameras
from a browser — on your own server or NAS, with no cloud and no vendor app.

- **Live view** — low-latency grid (1 / 2×2 / 3×3 / 4×4) and single-camera
  view with sound, fullscreen wall, snapshots.
- **Recording** — continuous, scheduled, on events (with pre- and
  post-record) or manual; per-camera retention; clips play while they are
  still being recorded.
- **Motion** — the camera's own ONVIF events (motion, person, vehicle) or
  Watchgrid's software detection with sensitivity and drawn zones.
- **Events** — timeline, filters, protection from retention, notifications
  (in-app and webhook).
- **Exports** — download, or upload clips to any S3-compatible storage
  (MinIO, AWS, Backblaze B2, Wasabi, Cloudflare R2, Synology C2…) or
  Nextcloud/WebDAV, by hand or automatically.
- **Operations** — one static binary, PostgreSQL, one-file backups,
  HTTPS behind a reverse proxy, capacity estimate for your hardware.

Works with H.264 cameras over RTSP (Tapo, EZVIZ, Hikvision, Dahua, Reolink…);
camera audio (G.711) is played and recorded as Opus.

## How it's built

| Part | Technology |
|---|---|
| Server | Rust, Axum, Tokio, sqlx + PostgreSQL, retina (RTSP) |
| Web UI | Rust → WebAssembly with Leptos (no Node.js, no npm) |
| Video | camera H.264 repackaged as fragmented MP4 (no transcoding) |
| Audio | G.711 → Opus (pure Rust) |
| Motion | ONVIF events, or OpenH264 on the substream |

Everything builds with Cargo.

## Install

Pick one:

- **Docker (Synology DSM and other NAS)** — see
  [`deploy/docker/README.md`](deploy/docker/README.md). `setup.sh` creates the
  database (in your PostgreSQL container, or a bundled one) and starts
  everything.
- **Linux with systemd** — see [`deploy/README.md`](deploy/README.md).

Both need **PostgreSQL 14+**.

### Build from source

```sh
rustup target add wasm32-unknown-unknown
cargo build --release -p watchgrid-server   # the server: target/release/watchgrid
cargo web build --release --live            # the web UI: dist/
```

For the static binary used by the Docker image:

```sh
rustup target add x86_64-unknown-linux-musl
sudo apt install musl-tools clang             # OpenH264 is compiled with clang for musl
sh deploy/docker/package.sh                   # → target/watchgrid-docker.tar.gz
```

## First sign-in: create an account

Accounts are managed **only on the server** — the web UI can sign in and
out but never create users or reset passwords.

```sh
# Docker
sudo docker exec -it watchgrid watchgrid user create <username>

# systemd install
sudo watchgrid user create <username>
```

You are asked for the password in the terminal (it is never passed on the
command line). Add `--viewer` for a read-only account. Other commands:

```sh
watchgrid user list
watchgrid user passwd <username>      # new password
watchgrid user disable <username>     # or enable / delete
```

Then open `http://<server>:8090`, sign in and add your cameras
(**Cameras → Add camera**: RTSP address, user, password; optionally the
substream and ONVIF).

## Configuration

Settings live in `watchgrid.env` (Docker) or `/etc/watchgrid/watchgrid.env`
(systemd). Everything else is set in the web UI.

| Variable | Meaning |
|---|---|
| `DATABASE_URL` | `postgres://watchgrid:<password>@<host>:5432/watchgrid` |
| `WATCHGRID_BIND` | listen address, default `127.0.0.1:8090` (`0.0.0.0:8090` in Docker) |
| `WATCHGRID_DATA_DIR` | server state (default `./data`) |
| `WATCHGRID_KEY_FILE` | master key that encrypts camera and export passwords |
| `WATCHGRID_RECORDINGS_DIR` | default recordings folder |
| `WATCHGRID_BACKUP_DIR` | daily backups (default `<data dir>/backups`) |
| `WATCHGRID_TRUSTED_PROXIES` | reverse proxy address(es) for HTTPS, e.g. `127.0.0.1` |
| `WATCHGRID_SECURE_COOKIES` | `1` = Secure session cookies for everyone |
| `RUST_LOG` | overrides the log level set in the UI |

## HTTPS

Watchgrid serves plain HTTP; put a reverse proxy with a certificate in
front and set `WATCHGRID_TRUSTED_PROXIES` to its address. Ready-made nginx
configs (standalone and Synology DSM) are in [`deploy/nginx/`](deploy/nginx/).

## Backup and restore

A backup is one `.wgbackup` file with the database **and** the master key
(camera passwords can't be decrypted without it). Recordings are not
included — back up their folder with your NAS tools.

- Automatic: every day in the backup folder, the newest 7 kept.
- By hand: `watchgrid backup` (Docker: `sudo docker exec watchgrid watchgrid backup`).
- Restore, with Watchgrid stopped: `watchgrid restore <file>.wgbackup --replace`.

Backups from older versions restore into newer ones.

## Project layout

```
crates/server   the NVR server (API, RTSP, recording, motion, exports…)
crates/web      the web UI (Leptos, compiled to WebAssembly)
crates/model    types shared by both
xtask           `cargo web` — builds and serves the UI
deploy          systemd, Docker and nginx setups
progress        development log, one file per step
```

## Status

Used daily on a Synology NAS with Tapo and EZVIZ cameras. Planned: AI
object detection, Google Drive / Dropbox exports.

## License

[GNU Affero General Public License v3.0 or later](LICENSE). You may use,
study, change and share Watchgrid; if you offer a modified version to others
over a network, you must share its source too.

Bundled and used components keep their own licenses (OpenH264 BSD-2,
opus-pure BSD-3, retina MIT/Apache-2.0, and the crates in `Cargo.lock`).

