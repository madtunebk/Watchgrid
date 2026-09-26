//! `/api/v1/settings` endpoints.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use watchgrid_model::Settings;

use super::{app, applied};
use crate::bus::BusEvent;
use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(read).put(update))
}

async fn read(State(s): State<AppState>) -> ApiResult<Json<Settings>> {
    app::load(&s.db, s.bind).await.map(Json)
}

async fn update(State(s): State<AppState>, Json(input): Json<Settings>) -> ApiResult<Json<Settings>> {
    // Read what the runtime step needs first: a failure here changes nothing,
    // instead of leaving a new transport half applied.
    let cameras = crate::cameras::all_ids(&s).await?;
    let saved = app::save(&s.db, s.bind, input).await?;
    if applied::apply(&saved.advanced) {
        // New RTSP transport: reopen every camera stream with it.
        tracing::info!(transport = ?saved.advanced.rtsp_transport, "RTSP transport changed; reconnecting cameras");
        for (id, _) in cameras {
            s.media.reload(&id);
        }
    }
    s.bus.publish(BusEvent::SettingsChanged);
    Ok(Json(saved))
}
