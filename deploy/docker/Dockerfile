# Watchgrid in a minimal image: a static (musl) binary and the web UI.
# No shell, no package manager. Built by deploy/docker/package.sh.
FROM scratch

COPY watchgrid /usr/bin/watchgrid
COPY ui /usr/share/watchgrid/ui

ENV WATCHGRID_BIND=0.0.0.0:8090 \
    WATCHGRID_DATA_DIR=/var/lib/watchgrid \
    WATCHGRID_KEY_FILE=/etc/watchgrid/master.key \
    WATCHGRID_RECORDINGS_DIR=/recordings \
    WATCHGRID_UI_DIR=/usr/share/watchgrid/ui
# No RUST_LOG here: Settings → Advanced → Log level decides (RUST_LOG in
# watchgrid.env still overrides it).

# /etc/watchgrid: master key (back it up!) · /var/lib/watchgrid: state · /recordings: clips
VOLUME ["/etc/watchgrid", "/var/lib/watchgrid", "/recordings"]
EXPOSE 8090
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/bin/watchgrid"]
CMD ["serve"]
