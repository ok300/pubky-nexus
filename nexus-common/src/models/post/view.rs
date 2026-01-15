use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{Bookmark, PostCounts, PostDetails, PostRelationships};
use crate::db::RedisOps;
use crate::models::tag::post::TagPost;
use crate::models::tag::traits::TagCollection;
use crate::models::tag::TagDetails;
use crate::types::DynError;

/// Represents a Pubky user with relational data including tags, counts, and relationship with a viewer.
#[derive(Serialize, Deserialize, ToSchema, Default, Debug)]
pub struct PostView {
    pub details: PostDetails,
    pub counts: PostCounts,
    pub tags: Vec<TagDetails>,
    pub relationships: PostRelationships,
    pub bookmark: Option<Bookmark>,
}

impl PostView {
    /// Retrieves a user ID, checking the cache first and then the graph database.
    pub async fn get_by_id(
        author_id: &str,
        post_id: &str,
        viewer_id: Option<&str>,
        limit_tags: Option<usize>,
        limit_taggers: Option<usize>,
    ) -> Result<Option<Self>, DynError> {
        // Perform all operations concurrently
        let (details, counts, bookmark, relationships) = tokio::try_join!(
            PostDetails::get_by_id(author_id, post_id),
            PostCounts::get_by_id(author_id, post_id),
            Bookmark::get_by_id(author_id, post_id, viewer_id),
            PostRelationships::get_by_id(author_id, post_id),
        )?;

        let details = match details {
            None => return Ok(None),
            Some(details) => details,
        };

        let counts = counts.unwrap_or_default();
        let relationships = relationships.unwrap_or_default();

        // Before fetching post tags, check if the post has any tags
        // Without this check, the index search will return a NONE because the tag index
        // doesn't exist, leading us to query the graph unnecessarily, assuming the data wasn't indexed
        let tags = match counts.tags {
            0 => Vec::new(),
            _ => {
                TagPost::get_by_id(
                    author_id,
                    Some(post_id),
                    None,
                    limit_tags,
                    limit_taggers,
                    viewer_id,
                    None, // Avoid by default WoT tags in a Post
                )
                .await?
                .unwrap_or_default()
            }
        };

        Ok(Some(Self {
            details,
            counts,
            bookmark,
            relationships,
            tags,
        }))
    }

    /// Retrieves multiple posts by their keys using batch Redis operations for better performance.
    ///
    /// This method uses `mget` operations to fetch post details, counts, relationships, and
    /// bookmarks in bulk, significantly reducing the number of Redis round-trips compared to
    /// individual queries. For cache misses, it falls back to individual queries with graph lookup.
    ///
    /// # Arguments
    ///
    /// * `post_keys` - A slice of post keys in the format "author_id:post_id"
    /// * `viewer_id` - Optional viewer ID for bookmark lookups
    ///
    /// # Returns
    ///
    /// A vector of `Option<PostView>` corresponding to each post key
    pub async fn get_by_ids(
        post_keys: &[String],
        viewer_id: Option<&str>,
    ) -> Result<Vec<Option<Self>>, DynError> {
        if post_keys.is_empty() {
            return Ok(Vec::new());
        }

        // Build bookmark keys (author_id:post_id:viewer_id)
        let bookmark_keys: Option<Vec<String>> = viewer_id.map(|vid| {
            post_keys
                .iter()
                .map(|pk| format!("{}:{}", pk, vid))
                .collect()
        });

        // Use mget to batch fetch all data from Redis cache
        // PostDetails, PostCounts, PostRelationships all use the same key format (author_id:post_id)
        // Bookmarks use author_id:post_id:viewer_id
        let (details_list, counts_list, relationships_list, bookmarks_list) = tokio::try_join!(
            PostDetails::mget(post_keys),
            PostCounts::mget(post_keys),
            PostRelationships::mget(post_keys),
            async {
                match &bookmark_keys {
                    Some(keys) => Bookmark::mget(keys).await,
                    None => Ok(vec![None; post_keys.len()]),
                }
            }
        )?;

        let mut post_views = Vec::with_capacity(post_keys.len());

        for (i, post_key) in post_keys.iter().enumerate() {
            let (author_id, post_id) = post_key.split_once(':').unwrap_or_default();

            // Get details - if None in cache, try fetching from graph
            let details = match &details_list[i] {
                Some(d) => d.clone(),
                None => match PostDetails::get_by_id(author_id, post_id).await? {
                    Some(d) => d,
                    None => {
                        post_views.push(None);
                        continue;
                    }
                },
            };

            // Get counts - if None in cache, try fetching from graph
            let counts = match &counts_list[i] {
                Some(c) => c.clone(),
                None => PostCounts::get_by_id(author_id, post_id)
                    .await?
                    .unwrap_or_default(),
            };

            // Get relationships - if None in cache, try fetching from graph
            let relationships = match &relationships_list[i] {
                Some(r) => r.clone(),
                None => PostRelationships::get_by_id(author_id, post_id)
                    .await?
                    .unwrap_or_default(),
            };

            // Get bookmark - if None in cache and viewer provided, try fetching from graph
            let bookmark = match &bookmarks_list[i] {
                Some(b) => Some(b.clone()),
                None => {
                    if viewer_id.is_some() {
                        Bookmark::get_by_id(author_id, post_id, viewer_id).await?
                    } else {
                        None
                    }
                }
            };

            // Fetch tags only if the post has any
            let tags = match counts.tags {
                0 => Vec::new(),
                _ => {
                    TagPost::get_by_id(author_id, Some(post_id), None, None, None, viewer_id, None)
                        .await?
                        .unwrap_or_default()
                }
            };

            post_views.push(Some(PostView {
                details,
                counts,
                bookmark,
                relationships,
                tags,
            }));
        }

        Ok(post_views)
    }
}
