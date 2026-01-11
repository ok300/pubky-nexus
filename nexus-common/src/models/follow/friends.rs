use crate::db::graph::queries;
use crate::db::RedisOps;
use crate::types::DynError;
use async_trait::async_trait;
use neo4rs::Query;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::following::Following;
use super::traits::UserFollows;

#[derive(Serialize, Deserialize, ToSchema, Default, Debug)]
pub struct Friends(pub Vec<String>);

impl Friends {
    // Checks wjether user_a and user_b are friends
    pub async fn check(user_a_id: &str, user_b_id: &str) -> Result<bool, DynError> {
        let user_a_key_parts = &[user_a_id][..];
        let user_b_key_parts = &[user_b_id][..];

        let ((_, a_follows_b), (_, b_follows_a)) = tokio::try_join!(
            Following::check_set_member(user_a_key_parts, user_b_id),
            Following::check_set_member(user_b_key_parts, user_a_id),
        )?;

        Ok(a_follows_b && b_follows_a)
    }
}

impl AsRef<[String]> for Friends {
    fn as_ref(&self) -> &[String] {
        &self.0
    }
}

#[async_trait]
impl RedisOps for Friends {}

impl UserFollows for Friends {
    fn from_vec(vec: Vec<String>) -> Self {
        Self(vec)
    }

    fn get_query(user_id: &str, skip: Option<usize>, limit: Option<usize>) -> Query {
        queries::get::get_user_friends(user_id, skip, limit)
    }

    fn get_ids_field_name() -> &'static str {
        "friend_ids"
    }
}
