//! Minimal JSON-over-HTTP helpers. Errors carry the server's message.
//!
//! Identical GETs in flight at the same time are sent once and share the
//! answer (header, sidebar and page often ask for the same thing as a page
//! opens). A change (any other method) starts a new generation, so nobody
//! is handed an answer that began before their change.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use futures::FutureExt;
use futures::future::{LocalBoxFuture, Shared};
use gloo_net::http::{Request, RequestBuilder, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::api::{ApiError, ApiResult};

const BASE: &str = "/api/v1";

fn url(path: &str) -> String {
    format!("{BASE}{path}")
}

fn unreachable(_: gloo_net::Error) -> ApiError {
    ApiError::new(0, "network", "Cannot reach the Watchgrid server")
}

async fn failure(resp: Response) -> ApiError {
    let status = resp.status();
    let error = resp.json::<ApiError>().await.unwrap_or_else(|_| ApiError::new(status, "http", format!("Request failed ({status})")));
    if status == 401 && error.code == "unauthenticated" {
        crate::api::session::session_expired();
    }
    error
}

async fn json<T: DeserializeOwned>(resp: Response) -> ApiResult<T> {
    if !resp.ok() {
        return Err(failure(resp).await);
    }
    resp.json::<T>().await.map_err(|e| ApiError::new(0, "decode", format!("Unexpected response: {e}")))
}

async fn send(req: RequestBuilder, body: Option<&impl Serialize>) -> ApiResult<Response> {
    let req = match body {
        Some(b) => req.json(b).map_err(unreachable)?,
        None => req.build().map_err(unreachable)?,
    };
    req.send().await.map_err(unreachable)
}

type Answer = Shared<LocalBoxFuture<'static, ApiResult<String>>>;

thread_local! {
    static IN_FLIGHT: RefCell<HashMap<(u64, String), Answer>> = RefCell::new(HashMap::new());
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// Called before every change: later GETs don't join earlier ones.
fn changing() {
    GENERATION.with(|g| g.set(g.get() + 1));
}

/// The body of `GET path`, shared with an identical request in flight.
async fn get_text(path: &str) -> ApiResult<String> {
    let key = (GENERATION.with(Cell::get), path.to_string());
    let answer = IN_FLIGHT.with(|m| {
        m.borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| {
                let (key, url) = (key.clone(), url(path));
                async move {
                    let result = async {
                        let resp = send(Request::get(&url), None::<&()>).await?;
                        if !resp.ok() {
                            return Err(failure(resp).await);
                        }
                        resp.text().await.map_err(unreachable)
                    }
                    .await;
                    IN_FLIGHT.with(|m| m.borrow_mut().remove(&key));
                    result
                }
                .boxed_local()
                .shared()
            })
            .clone()
    });
    answer.await
}

pub async fn get<T: DeserializeOwned>(path: &str) -> ApiResult<T> {
    let text = get_text(path).await?;
    serde_json::from_str(&text).map_err(|e| ApiError::new(0, "decode", format!("Unexpected response: {e}")))
}

pub async fn post<T: DeserializeOwned>(path: &str, body: Option<&impl Serialize>) -> ApiResult<T> {
    changing();
    json(send(Request::post(&url(path)), body).await?).await
}

pub async fn put<T: DeserializeOwned>(path: &str, body: &impl Serialize) -> ApiResult<T> {
    changing();
    json(send(Request::put(&url(path)), Some(body)).await?).await
}

/// POST without a body for endpoints that answer `204 No Content`.
pub async fn post_no_content(path: &str) -> ApiResult<()> {
    changing();
    let resp = send(Request::post(&url(path)), None::<&()>).await?;
    if resp.ok() { Ok(()) } else { Err(failure(resp).await) }
}

/// POST with a JSON body for endpoints that answer `204 No Content`.
pub async fn post_json_no_content(path: &str, body: &impl Serialize) -> ApiResult<()> {
    changing();
    let resp = send(Request::post(&url(path)), Some(body)).await?;
    if resp.ok() { Ok(()) } else { Err(failure(resp).await) }
}

/// PUT for endpoints that answer `204 No Content`.
pub async fn put_no_content(path: &str, body: &impl Serialize) -> ApiResult<()> {
    changing();
    let resp = send(Request::put(&url(path)), Some(body)).await?;
    if resp.ok() { Ok(()) } else { Err(failure(resp).await) }
}

pub async fn delete(path: &str) -> ApiResult<()> {
    changing();
    let resp = send(Request::delete(&url(path)), None::<&()>).await?;
    if resp.ok() { Ok(()) } else { Err(failure(resp).await) }
}
