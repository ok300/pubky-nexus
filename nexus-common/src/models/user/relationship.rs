use crate::db::{get_redis_conn, RedisOps};
use crate::models::follow::{Followers, UserFollows};
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

    /// Retrieves relationships for multiple users using batch Redis operations.
    pub async fn get_by_ids(
        user_ids: &[String],
        viewer_id: Option<&str>,
    ) -> Result<Vec<Option<Self>>, DynError> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let Some(viewer_id) = viewer_id else {
            return Ok(std::iter::repeat_with(|| None)
                .take(user_ids.len())
                .collect());
        };

        let viewer_exists = UserCounts::get_from_index(viewer_id).await?.is_some();
        if !viewer_exists {
            return Ok(std::iter::repeat_with(|| None)
                .take(user_ids.len())
                .collect());
        }

        let user_counts = UserCounts::mget(user_ids).await?;
        let followers_prefix = Followers::prefix().await;
        let muted_prefix = Muted::prefix().await;
        let viewer_followers_key = format!("{followers_prefix}:{viewer_id}");
        let viewer_muted_key = format!("{muted_prefix}:{viewer_id}");

        let mut redis_conn = get_redis_conn().await?;
        let mut pipe = redis::pipe();

        for user_id in user_ids {
            let user_followers_key = format!("{followers_prefix}:{user_id}");
            pipe.sismember(user_followers_key, viewer_id);
            pipe.sismember(&viewer_followers_key, user_id);
            pipe.sismember(&viewer_muted_key, user_id);
        }

        let results: Vec<bool> = pipe.query_async(&mut redis_conn).await?;
        let mut relationships = Vec::with_capacity(user_ids.len());
        let mut result_index = 0;

        for (i, _user_id) in user_ids.iter().enumerate() {
            if user_counts
                .get(i)
                .and_then(|count| count.as_ref())
                .is_none()
            {
                relationships.push(None);
            } else {
                let following = results[result_index];
                let followed_by = results[result_index + 1];
                let muted = results[result_index + 2];
                relationships.push(Some(Self {
                    following,
                    followed_by,
                    muted,
                }));
            }
            result_index += 3;
        }

        Ok(relationships)
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
