# Watchgrid with Docker (Synology DSM and other NAS)

A ~7 MB image (static binary + web UI, no OS inside) next to your
existing PostgreSQL.

## Quick path: existing PostgreSQL container

```sh
head -c 24 /dev/urandom | base64 | tr -d '/+=\n' > db-password   # the watchgrid DB user's password
sudo sh setup.sh
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

Together: the database (`pg_dump`), `./config/master.key` (camera passwords
can't be decrypted without it) and, optionally, the recordings folder.

## HTTPS

DSM → Control Panel → Login Portal → Reverse Proxy: forward
`https://nvr.example` to `http://localhost:8090`, enable WebSocket headers,
then set `WATCHGRID_SECURE_COOKIES=1`.
