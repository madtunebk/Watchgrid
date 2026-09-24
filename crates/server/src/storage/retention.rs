//! The retention policy: how long and how much footage to keep.

use sqlx::PgPool;
use watchgrid_model::RetentionPolicy;

use crate::error::{ApiError, ApiResult};
use crate::settings;

const KEY: &str = "retention";

/// Until the administrator sets rules, nothing is ever deleted.
pub fn default_policy() -> RetentionPolicy {
    RetentionPolicy { max_age_days: None, max_usage: None, min_free: None }
}

pub async fn load(db: &PgPool) -> ApiResult<RetentionPolicy> {
    Ok(settings::load(db, KEY).await?.unwrap_or_else(default_policy))
}

pub async fn save(db: &PgPool, policy: &RetentionPolicy) -> ApiResult<()> {
    validate(policy)?;
    settings::save(db, KEY, policy).await?;
    tracing::info!(?policy, "retention policy updated");
    Ok(())
}

fn validate(p: &RetentionPolicy) -> ApiResult<()> {
    if p.max_age_days == Some(0) {
        return Err(ApiError::invalid("Keep recordings for at least 1 day"));
    }
    if p.max_usage == Some(0) {
        return Err(ApiError::invalid("The storage limit must be larger than zero"));
    }
    Ok(())
}
