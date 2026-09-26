# Installing Watchgrid

Watchgrid is one Rust binary, a folder of static web files and a
PostgreSQL database. No Node.js, no Docker required.

## 1. PostgreSQL

Any PostgreSQL 14+ works (the distribution package is fine):

```sh
sudo -u postgres createuser --pwprompt watchgrid      # choose a password
sudo -u postgres createdb --owner watchgrid watchgrid
```

## 2. Build

```sh
cargo build --release -p watchgrid-server
cargo web build --release --live
```

## 3. Install

```sh
sudo sh deploy/install.sh            # first run creates /etc/watchgrid/watchgrid.env
sudoedit /etc/watchgrid/watchgrid.env   # set DATABASE_URL (and WATCHGRID_BIND)
sudo sh deploy/install.sh            # installs, initializes and starts the service
sudo watchgrid user create <username>
```

Open `http://<nas-address>:8090` and sign in.

## Where things live

| Path | What | Owner / mode |
|---|---|---|
| `/usr/bin/watchgrid` | the program | root, 0755 |
| `/usr/share/watchgrid/ui/` | web UI | root, read-only |
| `/etc/watchgrid/watchgrid.env` | settings incl. the database password | root:watchgrid 0640 |
| `/etc/watchgrid/master.key` | encrypts camera passwords | root:watchgrid 0640 |
| `/var/lib/watchgrid/` | server state | watchgrid 0750 |
| `/var/lib/watchgrid/recordings/` | default recordings folder | watchgrid |
| logs | `journalctl -u watchgrid` | |

The service runs as the unprivileged `watchgrid` user in a systemd
sandbox: it can read its config but not change it, and it can only
write to its state folder and the recording folders you allow.

## Recordings on another disk

```sh
sudo watchgrid storage set-path /volume1/watchgrid
```

As root this creates the folder, gives it to `watchgrid`, allows it in the
sandbox (`/etc/systemd/system/watchgrid.service.d/recordings.conf`) and
restarts the service (running recordings are finalized first). New
recordings go there; existing clips stay where they are and keep playing.

From the web UI (Settings → Storage) an administrator can switch between
folders the service can already write to.

`sudo watchgrid storage show` prints the current folder and free space.

## Accounts

Only from the server: `sudo watchgrid user create|list|passwd|enable|disable|delete <name>`.
Add `--viewer` to `create` for a read-only account.

## Backup

A backup is one `.wgbackup` file with the database **and** the master key
(camera passwords can't be decrypted without it):

- automatic: every day in `/var/lib/watchgrid/backups/` (the newest 7
  `watchgrid-auto-*` files are kept);
- by hand: `sudo watchgrid backup [--out FILE]`.

Recordings are not included — back up their folders separately if you want
them.

## Restore

```sh
sudo sh deploy/install.sh                       # program, user, service
sudo systemctl stop watchgrid
sudo watchgrid restore watchgrid-….wgbackup --replace
sudo systemctl start watchgrid
```

A backup from an older version restores fine; the database is upgraded
afterwards.

## Upgrade

Build again and re-run `sudo sh deploy/install.sh`. Database migrations
run automatically; settings, key and recordings are kept.

## HTTPS

Put Watchgrid behind a reverse proxy with TLS (Caddy, nginx, Traefik; see
`deploy/nginx/`) and set `WATCHGRID_TRUSTED_PROXIES` to its address (e.g.
`127.0.0.1`). The proxy must pass WebSocket upgrades for `/api/v1/ws` and
`/api/v1/cameras/*/live`, and `X-Forwarded-For` / `X-Forwarded-Proto`.
Session cookies then become Secure for HTTPS visitors automatically;
`WATCHGRID_SECURE_COOKIES=1` forces it for everyone.
