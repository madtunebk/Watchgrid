# Deploying Watchgrid

Pick the setup that fits your machine:

| Folder | For | Start with |
|---|---|---|
| [`docker/`](docker/) | Synology DSM and other NAS, or any Docker host | [`docker/README.md`](docker/README.md) |
| [`systemd/`](systemd/) | a Linux server, installed as a system service | [`systemd/README.md`](systemd/README.md) |
| [`nginx/`](nginx/) | HTTPS in front of either (standalone nginx or Synology DSM's) | the comments at the top of each `.conf` |

Both need PostgreSQL 14+. Accounts are created on the server with
`watchgrid user create <name>` (Docker: through `docker exec`).
