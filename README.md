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

Works with H.264 and HEVC/H.265 (`hvc1`) cameras over RTSP (Tapo, EZVIZ,
Hikvision, Dahua, Reolink…); camera audio (G.711) is played and recorded as
Opus. HEVC playback requires browser/device codec support; streams are not
transcoded. Software motion detection needs an H.264 substream, and event
thumbnails are currently generated only from H.264 recordings. Camera ONVIF
motion events can still trigger HEVC recordings. Video timing assumes camera
streams without B-frame reordering.

## How it's built

| Part | Technology |
|---|---|
| Server | Rust, Axum, Tokio, sqlx + PostgreSQL, retina (RTSP) |
| Web UI | Rust → WebAssembly with Leptos (no Node.js, no npm) |
| Video | camera H.264 or HEVC repackaged as MP4 (no transcoding) |
| Audio | G.711 → Opus (pure Rust) |
| Motion | ONVIF events, or OpenH264 on the substream |

Everything builds with Cargo.

## Install

Two ways, both with **PostgreSQL 14+**. Choose one. Ready-built archives
for x86-64 Linux are on the [Releases](../../releases) page (what changed:
[`CHANGELOG.md`](CHANGELOG.md)); you can also build them yourself.

### A. Docker (Synology DSM, other NAS, any Docker host)

The script to run is **`setup.sh`**, from the bundle.

1. **Get the bundle**: download `watchgrid-<version>-docker.tar.gz` from
   Releases, or build it on a Linux machine with Rust (see
   [Build from source](#build-from-source)):
   ```sh
   sh deploy/docker/package.sh          # → target/watchgrid-docker.tar.gz
   ```
2. **Copy it to the NAS** and unpack it into its own folder:
   ```sh
   mkdir -p ~/watchgrid && cd ~/watchgrid
   tar -xzf watchgrid-*docker.tar.gz --strip-components=1
   ```
3. **Choose the database password** (only the first time):
   ```sh
   head -c 24 /dev/urandom | base64 | tr -d '/+=\n' > db-password
   ```
4. **Run the setup** — it uses your PostgreSQL container, or starts its own:
   ```sh
   sudo sh setup.sh
   ```
5. **Create your account**, then open `http://<nas>:8090`:
   ```sh
   sudo docker exec -it watchgrid watchgrid user create <username>
   ```

**Upgrade:** download or build a new bundle, unpack it over the same folder, run
`sudo sh setup.sh` again. Details: [`deploy/docker/README.md`](deploy/docker/README.md).

### B. Bare metal (Linux server with systemd)

The script to run is **`deploy/systemd/install.sh`**, from the unpacked
release archive or the source folder.

1. **Create the database:**
   ```sh
   sudo -u postgres createuser --pwprompt watchgrid
   sudo -u postgres createdb --owner watchgrid watchgrid
   ```
2. **Get the program**: download `watchgrid-<version>-linux-x86_64.tar.gz`
   from Releases and unpack it (`tar -xzf …; cd watchgrid-*/`), or build it
   (see [Build from source](#build-from-source)):
   ```sh
   cargo build --release -p watchgrid-server
   cargo web build --release --live
   ```
3. **Install** — the first run creates the settings file:
   ```sh
   sudo sh deploy/systemd/install.sh
   sudoedit /etc/watchgrid/watchgrid.env      # DATABASE_URL, WATCHGRID_BIND
   sudo sh deploy/systemd/install.sh          # installs and starts the service
   ```
4. **Create your account**, then open `http://<server>:8090`:
   ```sh
   sudo watchgrid user create <username>
   ```

**Upgrade:** unpack the new release (or pull and build again), then
`sudo sh deploy/systemd/install.sh`.
Details: [`deploy/systemd/README.md`](deploy/systemd/README.md).

### Build from source

Needs Rust (stable), the WebAssembly target and a C/C++ compiler
(`build-essential` on Debian/Ubuntu) — no Node.js. Optional: `nasm` makes
the software motion detector's video decoding faster.

```sh
rustup target add wasm32-unknown-unknown
cargo build --release -p watchgrid-server   # the server: target/release/watchgrid
cargo web build --release --live            # the web UI: dist/
```

The Docker bundle uses a fully static binary; for that also:

```sh
rustup target add x86_64-unknown-linux-musl
sudo apt install musl-tools clang            # OpenH264 is compiled with clang for musl
```

## Accounts and passwords

Accounts are managed **only on the server** — the web UI can sign in and
out but never create users or reset passwords. The password is asked in
the terminal (never passed on the command line). Add `--viewer` to
`create` for a read-only account. With Docker, prefix the commands with
`sudo docker exec -it watchgrid`; on bare metal, with `sudo`:

```sh
watchgrid user create <username>      # add --viewer for read-only
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
deploy          docker/, systemd/ and nginx/ setups; release.sh builds the release archives
progress        development log, one file per step
```

## Status and roadmap

Used daily on a Synology NAS with Tapo and EZVIZ cameras. Right now the
focus is on polishing the Linux version until it is stable. After that:

- **Phone notifications** — ntfy and Telegram.
- **More platforms** — Linux ARM64 (Raspberry Pi 4/5, ARM NAS), then a
  native **Windows** version. The web UI already runs in any browser; the
  server needs porting. Until then, Windows hosts can use Docker.
- **Later** — AI object detection for cameras without their own, Google
  Drive / Dropbox exports.

## License

[GNU Affero General Public License v3.0 or later](LICENSE). You may use,
study, change and share Watchgrid; if you offer a modified version to others
over a network, you must share its source too.

Bundled and used components keep their own licenses (OpenH264 BSD-2,
opus-pure BSD-3, retina MIT/Apache-2.0, and the crates in `Cargo.lock`).
