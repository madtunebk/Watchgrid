use super::{ApiResult, Id, Session, User, backend};

/// GET /api/v1/users
pub async fn get_users() -> ApiResult<Vec<User>> {
    backend::auth::users().await
}

/// GET /api/v1/sessions
pub async fn get_sessions() -> ApiResult<Vec<Session>> {
    backend::auth::sessions().await
}

/// DELETE /api/v1/sessions/{id} — sign that device out.
pub async fn revoke_session(id: Id) -> ApiResult<()> {
    backend::auth::revoke(&id).await
}
