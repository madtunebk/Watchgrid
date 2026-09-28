# Changelog

Each version's section here becomes the notes of its GitHub Release.

## v0.2.0

Armed mode, event dots on the timeline, HEVC, and a long list of fixes that
make Watchgrid say what is really going on.

- **Armed mode** — a lock in the header, next to the bell, on every page.
  Arm all cameras or just the ones you pick: they detect motion themselves
  and record on it. Disarm puts each camera back as it was (changes made by
  hand in between stay).
- **Motion** — Watchgrid's own detection is now the recommended source and
  the default for new cameras (on the substream); it works the same with
  every camera. ONVIF events stay optional, and while a camera's events
  don't arrive, software detection stands in. The Motion tab says whether
  detection really works.
- **Timeline** — events are dots on each camera's row, coloured by kind;
  a click plays from just before the event. Clicking a clip plays that clip;
  clips start quickly and the player says when it is loading or why it can't.
- **HEVC / H.265** — live view and recordings without transcoding (where
  the browser can play HEVC).
- **Events and recordings** — select many, or "select all N matching", to
  protect or delete; pages instead of endless lists; still images for
  events; one motion is one event instead of many short ones; deleting a
  recording also removes its events; an event without video has its own page.
- **Notifications** — a Notifications page with filters, read/unread per
  user, a test button, motion notifications chosen per camera, webhooks over
  HTTPS.
- **Exports** — failed uploads retry by themselves and can be cancelled;
  destinations can be edited and are checked live; deletions wait for
  uploads in progress.
- **Storage and retention** — retention in one place, with a preview of what
  would be deleted before you save, a separate rule for event history, and a
  projection based on how fast you actually record.
- **Recording** — recordings recover when internal messages are missed; a
  clip that fails while motion continues starts again.
- **Cameras** — "Add camera" lives in Settings → Cameras. PTZ says why when
  the camera doesn't answer. ONVIF: HTTP Digest login, and a refused login
  says why (no password saved, clock, wrong password).
- **Live view** — the grid and the camera's Live tab fill the page; controls
  and saved positions sit on the video.
- **Dashboard and health** — real totals, problems first, NVR health per
  component.
- **Operations** — backup and restore are command-line only (nothing in the
  web UI); restore checks the backup first and changes nothing if it fails.
  Stopping or upgrading the service releases the cameras' event
  subscriptions. The README lists system requirements.

Upgrading: install over v0.1.0 as usual; the database is updated when the
service starts.

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
- **Events** — timeline, filters, in-app and webhook notifications.
  Protecting an event also keeps its clip; deleting an event keeps the video,
  which has its own delete in Recordings.
- **Exports** — download, or upload to S3-compatible storage or
  Nextcloud/WebDAV, by hand or automatically.
- **Operations** — static binary, PostgreSQL, daily one-file backups and
  restore, HTTPS behind a reverse proxy, user management from the command line.
