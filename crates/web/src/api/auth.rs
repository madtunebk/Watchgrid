use super::{ApiResult, Id, Session, User, backend};

/// GET /api/v1/auth/users — read-only; accounts are managed from the server CLI.
pub async fn get_users() -> ApiResult<Vec<User>> {
    backend::auth::users().await
}

/// GET /api/v1/auth/sessions
pub async fn get_sessions() -> ApiResult<Vec<Session>> {
    backend::auth::sessions().await
}

/// DELETE /api/v1/auth/sessions/{id} — sign that device out.
pub async fn revoke_session(id: Id) -> ApiResult<()> {
    backend::auth::revoke(&id).await
}

/// GET /api/v1/auth/session — the signed-in user.
pub async fn current_user() -> ApiResult<User> {
    backend::auth::current().await
}

/// POST /api/v1/auth/login — sets an HttpOnly session cookie.
pub async fn login(username: String, password: String) -> ApiResult<User> {
    backend::auth::login(&username, &password).await
}

/// POST /api/v1/auth/logout
pub async fn logout() -> ApiResult<()> {
    backend::auth::logout().await
}
