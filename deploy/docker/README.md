# Watchgrid with Docker (Synology DSM and other NAS)

A ~7 MB image (static binary + web UI, no OS inside) next to your
existing PostgreSQL.

## Quick path

1. On a Linux machine with Rust, from the source folder:
   `sh deploy/docker/package.sh` → `target/watchgrid-docker.tar.gz`
   (needs `rustup target add x86_64-unknown-linux-musl`, `musl-tools`, `clang`).
2. On the NAS, in a folder of its own:

```sh
tar -xzf watchgrid-docker.tar.gz --strip-components=1
head -c 24 /dev/urandom | base64 | tr -d '/+=\n' > db-password   # the watchgrid DB user's password (first time only)
sudo sh setup.sh
sudo docker exec -it watchgrid watchgrid user create <username>
```

`setup.sh` uses a running PostgreSQL container (or `PG_CONTAINER=<name>`) —
or, if there is none, bundles one (`watchgrid-db`, postgres:16-alpine, data in
`./db`). With an existing container it creates the `watchgrid` user and
database, picks how to reach it (published port → host network, otherwise
the container's network), writes `watchgrid.env` + `compose.nas.yml`, builds
and starts. Safe to re-run (e.g. after an upgrade).

The manual steps below do the same by hand.

## 1. Database (in your PostgreSQL container)

```sh
sudo docker exec -it <postgres-container> psql -U postgres \
  -c "CREATE ROLE watchgrid LOGIN PASSWORD 'choose-a-password';" \
  -c "CREATE DATABASE watchgrid OWNER watchgrid;"
```

## 2. Files on the NAS

```sh
mkdir -p /volume1/docker/watchgrid && cd /volume1/docker/watchgrid
tar -xzf watchgrid-docker.tar.gz --strip-components=1
cp watchgrid.env.example watchgrid.env && vi watchgrid.env   # DATABASE_URL
mkdir -p config state /volume1/watchgrid                      # recordings folder
```

Recordings go to `./recordings` unless you set `RECORDINGS=/volume1/watchgrid`
(in a `.env` file next to `compose.yml`, or on the command line).

The container runs as your user (`PUID`/`PGID`, default 1026:100 — check
with `id`). The three folders must belong to that user.

## 3. Start

```sh
sudo RECORDINGS=/volume1/watchgrid docker compose up -d --build
sudo docker compose logs -f watchgrid
```

The first start creates the master key in `./config/master.key`.

## 4. First account (accounts exist only on the server)

```sh
sudo docker exec -it watchgrid watchgrid user create <username>
```

Open `http://<nas>:8090`.

## Everyday

| Task | Command |
|---|---|
| logs | `sudo docker compose logs -f watchgrid` |
| accounts | `sudo docker exec -it watchgrid watchgrid user list` (create, passwd, disable…) |
| storage | `sudo docker exec watchgrid watchgrid storage show` |
| upgrade | unpack the new bundle over the folder, then `sudo docker compose up -d --build` |

## Backup

A backup is one `.wgbackup` file: the database **and** the master key
(camera passwords and export secrets can't be decrypted without it). The
recordings folder is not included — cover it with the NAS's own backup.

- **Automatic:** every day, in `./state/backups/` (the newest 7
  `watchgrid-auto-*` files are kept). Include `./state/backups` in Hyper
  Backup or copy them off the NAS.
- **By hand**, e.g. before an upgrade:
  `sudo docker exec watchgrid watchgrid backup` → `./state/backups/watchgrid-<time>.wgbackup`
  (never rotated).
- **Restore** (Watchgrid must be stopped; `--replace` overwrites existing data):

  Run these in the Watchgrid folder (where `setup.sh` is, e.g. `cd ~/watchgrid`):
  `docker compose` finds its settings there.

  ```
  sudo docker compose stop watchgrid
  sudo docker compose run --rm watchgrid restore /var/lib/watchgrid/backups/<file>.wgbackup --replace
  sudo docker compose start watchgrid
  ```

  A backup made by an older version restores fine: the database is brought
  up to date afterwards. A different master key already in `./config` is
  kept as `master.key.replaced-<time>`.

The files hold the master key: keep them as private as `./config`.

## HTTPS

Watchgrid serves plain HTTP; a reverse proxy in front adds TLS.

- **nginx:** `deploy/nginx/watchgrid.conf` is a complete example (WebSocket
  upgrade for live video, long timeouts, no buffering for clips).
- **DSM's own nginx** (it holds ports 80/443): `deploy/nginx/watchgrid-dsm.conf`
  goes to `/etc/nginx/sites-enabled/watchgrid.conf`, then
  `sudo nginx -t && sudo systemctl reload nginx`.
- **DSM:** Control Panel → Login Portal → Reverse Proxy: forward
  `https://nvr.example` to `http://localhost:8090` and add the WebSocket
  custom headers.

Then, in `watchgrid.env`, and restart (`sudo docker compose up -d`):

```
WATCHGRID_TRUSTED_PROXIES=127.0.0.1     # the proxy's address as Watchgrid sees it
```

Without it every client appears as the proxy: the sessions list shows its
address, and a few wrong passwords from anyone lock sign-in for everybody.
`X-Forwarded-For` / `X-Forwarded-Proto` are only believed from the listed
addresses. Browsers that come over HTTPS get Secure session cookies
automatically, while `http://<nas>:8090` on the LAN keeps working;
`WATCHGRID_SECURE_COOKIES=1` forces Secure for everyone (plain HTTP sign-in
then stops working).
