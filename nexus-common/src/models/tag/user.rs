use crate::db::get_redis_conn;
use crate::db::kv::SortOrder;
use crate::db::RedisOps;
use crate::models::tag::traits::collection::{CACHE_SET_PREFIX, CACHE_SORTED_SET_PREFIX};
use crate::models::tag::TagDetails;
use crate::types::DynError;
use async_trait::async_trait;
use redis::Value;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::traits::{TagCollection, TaggersCollection};

pub const USER_TAGS_KEY_PARTS: [&str; 2] = ["Users", "Tag"];

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema, Default)]
pub struct TagUser(pub Vec<String>);

impl AsRef<[String]> for TagUser {
    fn as_ref(&self) -> &[String] {
        &self.0
    }
}

#[async_trait]
impl RedisOps for TagUser {
    async fn prefix() -> String {
        String::from("User:Taggers")
    }
}

impl TagCollection for TagUser {
    fn get_tag_prefix<'a>() -> [&'a str; 2] {
        USER_TAGS_KEY_PARTS
    }
}

impl TaggersCollection for TagUser {}

impl TagUser {
    /// Retrieves multiple tag collections for users using batch Redis operations.
    pub async fn get_by_ids(
        user_ids: &[String],
        extra_param: Option<&str>,
        skip_tags: Option<usize>,
        limit_tags: Option<usize>,
        limit_taggers: Option<usize>,
        viewer_id: Option<&str>,
        depth: Option<u8>,
    ) -> Result<Vec<Option<Vec<TagDetails>>>, DynError> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let use_cache = viewer_id.is_some() && matches!(depth, Some(1..=3));
        let index_extra_param = if use_cache { viewer_id } else { extra_param };

        let mut tag_details = Self::get_from_index_multiple(
            user_ids,
            index_extra_param,
            skip_tags,
            limit_tags,
            limit_taggers,
            viewer_id,
            use_cache,
        )
        .await?;

        let mut missing_ids = Vec::new();
        for (i, tags) in tag_details.iter().enumerate() {
            if tags.is_none() {
                missing_ids.push(i);
            }
        }

        for index in missing_ids {
            let user_id = &user_ids[index];
            let graph_tags = if use_cache {
                let depth = depth.unwrap_or(1);
                Self::get_from_graph(user_id, viewer_id, Some(depth)).await?
            } else {
                Self::get_from_graph(user_id, extra_param, None).await?
            };

            if let Some(tags) = graph_tags {
                Self::put_to_index(user_id, index_extra_param, &tags, use_cache).await?;
                tag_details[index] = Some(tags);
            }
        }

        Ok(tag_details)
    }

    async fn get_from_index_multiple(
        user_ids: &[String],
        extra_param: Option<&str>,
        skip_tags: Option<usize>,
        limit_tags: Option<usize>,
        limit_taggers: Option<usize>,
        viewer_id: Option<&str>,
        is_cache: bool,
    ) -> Result<Vec<Option<Vec<TagDetails>>>, DynError> {
        let skip_tags = skip_tags.unwrap_or(0);
        let limit_tags = limit_tags.unwrap_or(5);
        let limit_taggers = limit_taggers.unwrap_or(5);

        let sorted_prefix = if is_cache {
            CACHE_SORTED_SET_PREFIX
        } else {
            "Sorted"
        };
        let taggers_prefix = if is_cache {
            Some(CACHE_SET_PREFIX.to_string())
        } else {
            None
        };

        let sorted_keys: Vec<String> = user_ids
            .iter()
            .map(|user_id| {
                Self::create_sorted_set_key_parts(user_id, extra_param, is_cache).join(":")
            })
            .collect();

        let tag_scores_list = Self::get_multiple_sorted_sets(
            &sorted_keys,
            sorted_prefix,
            Some(skip_tags),
            Some(limit_tags),
            SortOrder::Descending,
        )
        .await?;

        let mut tag_keys_per_user = Vec::with_capacity(user_ids.len());
        let mut total_tag_keys = Vec::new();
        for (i, tag_scores) in tag_scores_list.iter().enumerate() {
            let mut tag_keys = Vec::new();
            if let Some(tag_scores) = tag_scores {
                for (label, score) in tag_scores.iter() {
                    if score >= &1.0 {
                        tag_keys.push(<Self as TagCollection>::create_label_index(
                            &user_ids[i],
                            extra_param,
                            label,
                            is_cache,
                        ));
                    }
                }
            }
            total_tag_keys.extend(tag_keys.iter().cloned());
            tag_keys_per_user.push(tag_keys);
        }

        let mut taggers_list = Vec::new();
        if !total_tag_keys.is_empty() {
            let tag_keys_ref: Vec<&str> = total_tag_keys.iter().map(String::as_str).collect();
            taggers_list = Self::try_from_multiple_sets(
                &tag_keys_ref,
                taggers_prefix,
                viewer_id,
                Some(limit_taggers),
            )
            .await?;
        }

        let mut tagger_index = 0;
        let mut tag_details_list = Vec::with_capacity(user_ids.len());
        for (i, tag_scores) in tag_scores_list.into_iter().enumerate() {
            match tag_scores {
                None => tag_details_list.push(None),
                Some(tag_scores) => {
                    let tag_keys_len = tag_keys_per_user[i].len();
                    if tag_keys_len == 0 {
                        tag_details_list.push(Some(Vec::new()));
                        continue;
                    }

                    let taggers_slice =
                        taggers_list[tagger_index..tagger_index + tag_keys_len].to_vec();
                    tagger_index += tag_keys_len;
                    tag_details_list.push(Some(TagDetails::from_index(tag_scores, taggers_slice)));
                }
            }
        }

        Ok(tag_details_list)
    }

    async fn get_multiple_sorted_sets(
        keys: &[String],
        prefix: &str,
        skip: Option<usize>,
        limit: Option<usize>,
        sorting: SortOrder,
    ) -> Result<Vec<Option<Vec<(String, f64)>>>, DynError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }

        let mut redis_conn = get_redis_conn().await?;
        let mut pipe = redis::pipe();

        let min_score = f64::MIN;
        let max_score = f64::MAX;
        let skip = skip.unwrap_or(0) as isize;
        let limit = limit.unwrap_or(1000) as isize;

        for key in keys {
            let index_key = format!("{prefix}:{key}");
            pipe.exists(&index_key);
            match sorting {
                SortOrder::Ascending => {
                    pipe.cmd("ZRANGEBYSCORE")
                        .arg(&index_key)
                        .arg(min_score)
                        .arg(max_score)
                        .arg("WITHSCORES")
                        .arg("LIMIT")
                        .arg(skip)
                        .arg(limit);
                }
                SortOrder::Descending => {
                    pipe.cmd("ZREVRANGEBYSCORE")
                        .arg(&index_key)
                        .arg(max_score)
                        .arg(min_score)
                        .arg("WITHSCORES")
                        .arg("LIMIT")
                        .arg(skip)
                        .arg(limit);
                }
            }
        }

        let results: Vec<Value> = pipe.query_async(&mut redis_conn).await?;
        let mut tag_scores_list = Vec::with_capacity(keys.len());
        let mut results_iter = results.into_iter();

        for _ in keys {
            let exists_value = results_iter.next().unwrap_or(Value::Nil);
            let exists: bool = redis::from_redis_value(&exists_value)?;
            let range_value = results_iter.next().unwrap_or(Value::Nil);

            if !exists {
                tag_scores_list.push(None);
                continue;
            }

            let range: Vec<(String, f64)> = redis::from_redis_value(&range_value)?;
            tag_scores_list.push(Some(range));
        }

        Ok(tag_scores_list)
    }
}
