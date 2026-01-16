use crate::db::{check_members_batch, RedisOps};
use crate::models::follow::Followers;
use crate::models::user::Muted;

use super::UserCounts;
use crate::types::DynError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Represents the relationship of the user that views and user being viewed.
#[derive(Serialize, Deserialize, ToSchema, Debug, Default)]
pub struct Relationship {
    pub following: bool,
    pub followed_by: bool,
    pub muted: bool,
}

impl Relationship {
    // Retrieves user-viewer relationship
    pub async fn get_by_id(
        user_id: &str,
        viewer_id: Option<&str>,
    ) -> Result<Option<Self>, DynError> {
        match viewer_id {
            None => Ok(None),
            Some(v_id) => Self::get_from_index(user_id, v_id).await,
        }
    }

    /// Retrieves relationship from Followers/Following Redis index sets.
    ///
    /// This function is optimized to use batched Redis operations:
    /// - Single MGET call to check both users exist (instead of 2 separate calls)
    /// - Single pipeline with 3 SISMEMBER calls for relationship checks (instead of 3 separate calls)
    ///
    /// Total: 2 Redis round-trips instead of 5.
    pub async fn get_from_index(
        user_id: &str,
        viewer_id: &str,
    ) -> Result<Option<Relationship>, DynError> {
        // Batch check: verify both users exist in a single Redis call
        let user_counts_results =
            UserCounts::try_from_index_multiple_json(&[&[user_id], &[viewer_id]]).await?;

        // Make sure both users exist before getting their relationship
        if user_counts_results.iter().any(|r| r.is_none()) {
            return Ok(None);
        }

        // Get prefixes for batched set membership checks
        let followers_prefix = Followers::prefix().await;
        let muted_prefix = Muted::prefix().await;

        // Batch all set membership checks in a single Redis pipeline call:
        // 1. Check if viewer follows user (following)
        // 2. Check if user follows viewer (followed_by)
        // 3. Check if viewer muted user (muted)
        let checks = [
            (followers_prefix.as_str(), user_id, viewer_id),   // following
            (followers_prefix.as_str(), viewer_id, user_id),   // followed_by
            (muted_prefix.as_str(), viewer_id, user_id),       // muted
        ];

        let results = check_members_batch(&checks).await?;

        Ok(Some(Self {
            following: results.first().copied().unwrap_or(false),
            followed_by: results.get(1).copied().unwrap_or(false),
            muted: results.get(2).copied().unwrap_or(false),
        }))
    }
}
