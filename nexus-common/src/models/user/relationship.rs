use crate::models::follow::{Followers, UserFollows};
use crate::models::user::Muted;

use super::UserCounts;
use crate::db::RedisOps;
use crate::types::DynError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Represents the relationship of the user that views and user being viewed.
#[derive(Serialize, Deserialize, ToSchema, Debug, Default, Clone)]
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

    /// Retrieves relationships for multiple users with a single viewer using batch Redis operations.
    ///
    /// This method uses Redis pipelines to check all follow/mute relationships in bulk,
    /// significantly improving performance when retrieving relationships for multiple users.
    ///
    /// # Arguments
    ///
    /// * `user_ids` - A slice of user IDs to get relationships for
    /// * `viewer_id` - An optional viewer ID. If None, returns None for all users.
    ///
    /// # Returns
    ///
    /// A vector of Option<Relationship> corresponding to each user_id.
    /// Returns None for users that don't exist.
    pub async fn get_by_ids(
        user_ids: &[String],
        viewer_id: Option<&str>,
    ) -> Result<Vec<Option<Self>>, DynError> {
        let Some(viewer_id) = viewer_id else {
            return Ok(vec![None; user_ids.len()]);
        };

        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Batch check user existence using UserCounts::mget
        let counts_list = UserCounts::mget(user_ids).await?;

        // Also check if viewer exists
        let viewer_exists = UserCounts::get_from_index(viewer_id).await?.is_some();
        if !viewer_exists {
            return Ok(vec![None; user_ids.len()]);
        }

        // Find which users exist (have counts in the index)
        let existing_users: Vec<(usize, &String)> = user_ids
            .iter()
            .enumerate()
            .filter(|(i, _)| counts_list[*i].is_some())
            .collect();

        if existing_users.is_empty() {
            return Ok(vec![None; user_ids.len()]);
        }

        // Build batch checks for Followers
        // For each existing user, we need to check:
        // 1. following: Does viewer follow user? (viewer_id's following set contains user_id)
        // 2. followed_by: Does user follow viewer? (user_id's following set contains viewer_id)
        let mut follower_checks: Vec<(&str, &str)> = Vec::with_capacity(existing_users.len() * 2);

        for (_, user_id) in &existing_users {
            // Check if user_id is in viewer's following set (following)
            follower_checks.push((viewer_id, user_id.as_str()));
            // Check if viewer_id is in user's following set (followed_by)
            follower_checks.push((user_id.as_str(), viewer_id));
        }

        // Build batch checks for Muted
        // For each existing user, check if user_id is in viewer's muted set
        let muted_checks: Vec<(&str, &str)> = existing_users
            .iter()
            .map(|(_, user_id)| (viewer_id, user_id.as_str()))
            .collect();

        // Execute batch checks
        let (follower_results, muted_results) = tokio::try_join!(
            Followers::check_set_members_batch(&follower_checks),
            Muted::check_set_members_batch(&muted_checks),
        )?;

        // Build result vector
        let mut results = vec![None; user_ids.len()];
        for (i, (original_idx, _)) in existing_users.iter().enumerate() {
            let following = follower_results[i * 2];
            let followed_by = follower_results[i * 2 + 1];
            let muted = muted_results[i];

            results[*original_idx] = Some(Self {
                following,
                followed_by,
                muted,
            });
        }

        Ok(results)
    }

    /// Retrieves relationship from Followers/Following Redis index sets.
    pub async fn get_from_index(
        user_id: &str,
        viewer_id: &str,
    ) -> Result<Option<Relationship>, DynError> {
        let user_exist = UserCounts::get_from_index(user_id).await?;
        let viewer_exist = UserCounts::get_from_index(viewer_id).await?;

        // Make sure users exist before get their relationship
        if user_exist.is_none() || viewer_exist.is_none() {
            return Ok(None);
        }

        let (following, followed_by, muted) = tokio::try_join!(
            Followers::check(user_id, viewer_id),
            Followers::check(viewer_id, user_id),
            Muted::check(viewer_id, user_id),
        )?;

        Ok(Some(Self {
            followed_by,
            following,
            muted,
        }))
    }
}
