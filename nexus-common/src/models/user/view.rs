use futures::future::try_join_all;
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
    /// significantly improving performance when retrieving multiple users by reducing the number
    /// of individual Redis/Neo4j queries.
    ///
    /// Optimizations:
    /// - Details and counts are fetched using batch mget operations
    /// - Relationships are fetched using a single batch pipeline operation
    /// - Tags are fetched concurrently for all users that have tags
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

        // Identify users that exist and have tags to fetch
        let users_with_tags: Vec<(usize, String)> = user_ids
            .iter()
            .enumerate()
            .filter_map(|(i, user_id)| {
                if details_list[i].is_some() {
                    let tag_count = counts_list[i].as_ref().map(|c| c.tags).unwrap_or(0);
                    if tag_count > 0 {
                        return Some((i, user_id.clone()));
                    }
                }
                None
            })
            .collect();

        // Fetch tags concurrently for all users that have tags
        let tags_results: Vec<Option<Vec<TagDetails>>> = if users_with_tags.is_empty() {
            Vec::new()
        } else {
            let tag_futures = users_with_tags.iter().map(|(_, user_id)| {
                TagUser::get_by_id(user_id, None, None, None, None, viewer_id, depth)
            });
            try_join_all(tag_futures).await?
        };

        // Create a map of index -> tags for quick lookup
        let mut tags_map: Vec<Option<Vec<TagDetails>>> = vec![None; user_ids.len()];
        for (i, (original_idx, _)) in users_with_tags.iter().enumerate() {
            tags_map[*original_idx] = tags_results[i].clone();
        }

        // Build final result
        let mut user_views = Vec::with_capacity(user_ids.len());
        for i in 0..user_ids.len() {
            let details = &details_list[i];

            let Some(details) = details else {
                user_views.push(None);
                continue;
            };

            let counts = counts_list[i].clone().unwrap_or_default();
            let relationship = relationships_list[i].clone().unwrap_or_default();
            let tags = tags_map[i].take().unwrap_or_default();

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
