//! Minimal JSON-over-HTTP helpers. Errors carry the server's message.

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
    resp.json::<ApiError>().await.unwrap_or_else(|_| ApiError::new(status, "http", format!("Request failed ({status})")))
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

pub async fn get<T: DeserializeOwned>(path: &str) -> ApiResult<T> {
    json(send(Request::get(&url(path)), None::<&()>).await?).await
}

pub async fn post<T: DeserializeOwned>(path: &str, body: Option<&impl Serialize>) -> ApiResult<T> {
    json(send(Request::post(&url(path)), body).await?).await
}

pub async fn put<T: DeserializeOwned>(path: &str, body: &impl Serialize) -> ApiResult<T> {
    json(send(Request::put(&url(path)), Some(body)).await?).await
}

/// PUT for endpoints that answer `204 No Content`.
pub async fn put_no_content(path: &str, body: &impl Serialize) -> ApiResult<()> {
    let resp = send(Request::put(&url(path)), Some(body)).await?;
    if resp.ok() { Ok(()) } else { Err(failure(resp).await) }
}

pub async fn delete(path: &str) -> ApiResult<()> {
    let resp = send(Request::delete(&url(path)), None::<&()>).await?;
    if resp.ok() { Ok(()) } else { Err(failure(resp).await) }
}
