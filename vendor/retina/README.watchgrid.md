# retina 0.4.20 — Watchgrid patch

Unmodified upstream source of `retina` 0.4.20 (crates.io, MIT/Apache-2.0)
with one small patch in `src/client/mod.rs`, marked `[watchgrid patch]`:

1. An error reply that carries an *older* CSeq than the pending keepalive is
   taken as that keepalive's reply. Some camera firmwares echo the previous
   request's CSeq when rejecting a request (observed: `SET_PARAMETER` with
   CSeq 6 answered by `400 Bad Request` with CSeq 5); upstream treats that as
   an unexpected message and ends the session ("RTSP framing error:
   Unexpected RTSP response"). Success replies still need the exact CSeq.
   Replies are also accepted while the request is still `Flushing`.
2. When a `SET_PARAMETER` / `GET_PARAMETER` keepalive is rejected, later
   keepalives fall back to `OPTIONS` (seen on cameras that advertise
   SET_PARAMETER but answer 400 Bad Request).

Wired in via `[patch.crates-io]` in the workspace `Cargo.toml`. Drop this
directory once an upstream release contains equivalent fixes.
