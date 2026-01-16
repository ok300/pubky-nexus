use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{Relationship, UserCounts, UserDetails};
use crate::db::RedisOps;
use crate::models::tag::traits::TagCollection;
use crate::models::tag::user::TagUser;
use crate::models::tag::TagDetails;
use crate::types::DynError;

/// Represents a Pubky user with relational data including tags, counts, bookmark and relationship with other posts.
#[derive(Serialize, Deserialize, ToSchema, Default, Debug)]
pub struct UserView {
    pub details: UserDetails,
    pub counts: UserCounts,
    pub tags: Vec<TagDetails>,
    pub relationship: Relationship,
}

impl UserView {
    /// Retrieves a user by ID, checking the cache first and then the graph database.
    pub async fn get_by_id(
        user_id: &str,
        viewer_id: Option<&str>,
        depth: Option<u8>,
    ) -> Result<Option<Self>, DynError> {
        // Perform all operations concurrently
        let (details, counts, relationship) = tokio::try_join!(
            UserDetails::get_by_id(user_id),
            UserCounts::get_by_id(user_id),
            Relationship::get_by_id(user_id, viewer_id),
        )?;

        let Some(details) = details else {
            return Ok(None);
        };
        let counts = counts.unwrap_or_default();
        let relationship = relationship.unwrap_or_default();

        // Before fetching post tags, check if the post has any tags
        // Without this check, the index search will return a NONE because the tag index
        // doesn't exist, leading us to query the graph unnecessarily, assuming the data wasn't indexed
        let tags = match counts.tags {
            0 => Vec::new(),
            _ => TagUser::get_by_id(user_id, None, None, None, None, viewer_id, depth)
                .await?
                .unwrap_or_default(),
        };

        Ok(Some(Self {
            details,
            counts,
            relationship,
            tags,
        }))
    }

    /// Retrieves multiple users by their IDs using batch Redis operations for better performance.
    ///
    /// This method uses batch operations to fetch user details, counts, and relationships in bulk,
    /// reducing the number of individual Redis queries from O(n) to O(1) for relationships.
    pub async fn get_by_ids(
        user_ids: &[String],
        viewer_id: Option<&str>,
        depth: Option<u8>,
    ) -> Result<Vec<Option<Self>>, DynError> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Batch fetch: details, counts, and relationships in parallel
        let (details_list, counts_list, relationships_list) = tokio::try_join!(
            UserDetails::mget(user_ids),
            UserCounts::mget(user_ids),
            Relationship::get_by_ids(user_ids, viewer_id),
        )?;

        let mut user_views = Vec::with_capacity(user_ids.len());

        for (i, user_id) in user_ids.iter().enumerate() {
            let Some(details) = &details_list[i] else {
                user_views.push(None);
                continue;
            };

            let counts = counts_list[i].clone().unwrap_or_default();
            let relationship = relationships_list[i].clone().unwrap_or_default();

            // Fetch tags for users that have them
            let tags = match counts.tags {
                0 => Vec::new(),
                _ => TagUser::get_by_id(user_id, None, None, None, None, viewer_id, depth)
                    .await?
                    .unwrap_or_default(),
            };

            user_views.push(Some(Self {
                details: details.clone(),
                counts,
                relationship,
                tags,
            }));
        }

        Ok(user_views)
    }
}
