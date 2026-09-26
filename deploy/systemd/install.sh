#!/bin/sh
# Install (or upgrade) Watchgrid on this machine. Run from the repository
# root after building:
#   cargo build --release -p watchgrid-server
#   cargo web build --release --live
#   sudo sh deploy/systemd/install.sh
set -eu

BIN=target/release/watchgrid
# Built UI (override when staged elsewhere, e.g. UI=target/ui-release).
UI=${UI:-dist}
[ "$(id -u)" -eq 0 ] || { echo "run as root: sudo sh deploy/systemd/install.sh" >&2; exit 1; }
[ -x "$BIN" ] || { echo "missing $BIN — run: cargo build --release -p watchgrid-server" >&2; exit 1; }
[ -f "$UI/index.html" ] || { echo "missing $UI/ — run: cargo web build --release --live" >&2; exit 1; }

# Service account (no login, no home directory contents).
if ! id watchgrid >/dev/null 2>&1; then
    useradd --system --home-dir /var/lib/watchgrid --no-create-home --shell /usr/sbin/nologin watchgrid
    echo "created user watchgrid"
fi

# Program and UI.
install -m 0755 "$BIN" /usr/bin/watchgrid
rm -rf /usr/share/watchgrid/ui
mkdir -p /usr/share/watchgrid
cp -r "$UI" /usr/share/watchgrid/ui
chmod -R u=rwX,go=rX /usr/share/watchgrid
install -D -m 0644 deploy/systemd/README.md /usr/share/doc/watchgrid/README.md

# Config and secrets: root owns, the service may only read.
install -d -m 0750 -o root -g watchgrid /etc/watchgrid
if [ ! -f /etc/watchgrid/watchgrid.env ]; then
    install -m 0640 -o root -g watchgrid deploy/systemd/watchgrid.env.example /etc/watchgrid/watchgrid.env
    echo "created /etc/watchgrid/watchgrid.env — set DATABASE_URL, then run this script again"
    exit 0
fi

# State.
install -d -m 0750 -o watchgrid -g watchgrid /var/lib/watchgrid /var/lib/watchgrid/recordings

# Key + database schema (idempotent), then lock the key down.
/usr/bin/watchgrid init
chown root:watchgrid /etc/watchgrid/master.key
chmod 0640 /etc/watchgrid/master.key

# Service.
install -m 0644 deploy/systemd/watchgrid.service /etc/systemd/system/watchgrid.service
systemctl daemon-reload
systemctl enable watchgrid >/dev/null
systemctl restart watchgrid

echo
echo "Watchgrid is running: systemctl status watchgrid"
if ! /usr/bin/watchgrid user list 2>/dev/null | grep -qv "No Watchgrid users"; then
    echo "Create the first account:  sudo watchgrid user create <username>"
fi
