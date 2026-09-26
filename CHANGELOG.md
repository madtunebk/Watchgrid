# Changelog

Each version's section here becomes the notes of its GitHub Release.

## v0.1.0

First public release.

- **Live view** — grid (1 / 2×2 / 3×3 / 4×4) and single camera, fullscreen
  wall, snapshots, camera audio (G.711 played as Opus).
- **PTZ** — pan / tilt arrows and presets for ONVIF cameras that move.
- **Recording** — continuous, scheduled, on events (pre- and post-record) or
  manual, with audio; per-camera retention; clips play while they are still
  being recorded.
- **Playback** — DVR-style: click the timeline to play a camera from that
  moment, clip after clip.
- **Motion** — ONVIF events (motion, person, vehicle) or software detection
  on the substream, with sensitivity and drawn zones.
- **Events** — timeline, filters, protection from retention, in-app and
  webhook notifications.
- **Exports** — download, or upload to S3-compatible storage or
  Nextcloud/WebDAV, by hand or automatically.
- **Operations** — static binary, PostgreSQL, daily one-file backups and
  restore, HTTPS behind a reverse proxy, user management from the command line.
