use super::UserDetails;
use crate::db::RedisOps;
use crate::models::create_zero_score_tuples;
use crate::types::DynError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const USER_NAME_KEY_PARTS: [&str; 2] = ["Users", "Name"];
pub const USER_ID_KEY_PARTS: [&str; 2] = ["Users", "ID"];

/// List of user IDs
#[derive(Serialize, Deserialize, ToSchema, Default)]
pub struct UserSearch(pub Vec<String>);

impl RedisOps for UserSearch {}

impl UserSearch {
    pub async fn get_by_name(
        name_prefix: &str,
        skip: Option<usize>,
        limit: Option<usize>,
    ) -> Result<Option<Self>, DynError> {
        // Perform the lexicographical range search
        let elements = Self::get_from_index_name(name_prefix, skip, limit).await?;

        // If elements exist, process them to extract user_ids
        if let Some(elements) = elements {
            let user_ids: Vec<String> = elements
                .into_iter()
                .filter_map(|element| {
                    // Split by `:` and take the last part (user_id)
                    element.split(':').next_back().map(|p| p.to_string())
                })
                .collect();

            return Ok(Some(UserSearch(user_ids)));
        }

        Ok(None)
    }

    pub async fn get_by_id(
        id_prefix: &str,
        skip: Option<usize>,
        limit: Option<usize>,
    ) -> Result<Option<Self>, DynError> {
        // Perform the lexicographical range search
        let elements = Self::get_from_index_id(id_prefix, skip, limit).await?;

        Ok(elements.map(UserSearch))
    }

    pub async fn get_from_index_name(
        name_prefix: &str,
        skip: Option<usize>,
        limit: Option<usize>,
    ) -> Result<Option<Vec<String>>, DynError> {
        // Convert the username to lowercase to ensure case-insensitive search
        let name_prefix = name_prefix.to_lowercase();

        let min = format!("[{name_prefix}"); // Inclusive range starting with "name_prefix"
        let max = format!("({name_prefix}~"); // Exclusive range ending just after "name_prefix"

        // Perform the lexicographical range search
        Self::try_from_index_sorted_set_lex(&USER_NAME_KEY_PARTS, &min, &max, skip, limit).await
    }

    pub async fn get_from_index_id(
        id_prefix: &str,
        skip: Option<usize>,
        limit: Option<usize>,
    ) -> Result<Option<Vec<String>>, DynError> {
        let id_prefix = id_prefix.to_lowercase();

        let min = format!("[{id_prefix}"); // Inclusive range starting with "id_prefix"
        let max = format!("({id_prefix}~"); // Exclusive range ending just after "id_prefix"

        Self::try_from_index_sorted_set_lex(&USER_ID_KEY_PARTS, &min, &max, skip, limit).await
    }

    /// Adds multiple `user_id`s to Redis sorted sets:
    /// - using the username as index
    /// - using the user ID as index
    ///
    /// This method takes a list of `UserDetails` and adds them all to the sorted set at once.
    pub async fn put_to_index(details_list: &[&UserDetails]) -> Result<(), DynError> {
        // ensure existing records are deleted
        Self::delete_existing_records(
            details_list
                .iter()
                .map(|details| details.id.as_str())
                .collect::<Vec<&str>>()
                .as_slice(),
        )
        .await?;

        // Collect all the `username:user_id` pairs
        let mut pairs: Vec<String> = Vec::with_capacity(details_list.len());
        let mut ids: Vec<String> = Vec::with_capacity(details_list.len());

        for details in details_list.iter().filter(|d| d.name != "[DELETED]") {
            // Convert the username to lowercase before storing
            let username = details.name.to_lowercase();
            let user_id = &details.id;

            pairs.push(format!("{username}:{user_id}"));
            ids.push(user_id.to_string());
        }

        let pairs_zscore_tuples = create_zero_score_tuples(&pairs);
        Self::put_index_sorted_set(&USER_NAME_KEY_PARTS, &pairs_zscore_tuples, None, None).await?;
        let ids_zscore_tuples = create_zero_score_tuples(&ids);
        Self::put_index_sorted_set(&USER_ID_KEY_PARTS, &ids_zscore_tuples, None, None).await
    }

    async fn delete_existing_records(user_ids: &[&str]) -> Result<(), DynError> {
        if user_ids.is_empty() {
            return Ok(());
        }
        
        let mut records_to_delete: Vec<String> = Vec::new();
        
        // For each user_id, scan the sorted set directly to find ALL entries
        // This is more reliable than relying on the JSON index which may be stale
        for user_id in user_ids {
            if let Some(entries) = Self::find_entries_by_user_id(user_id).await? {
                records_to_delete.extend(entries);
            }
        }

        Self::remove_from_index_sorted_set(
            None,
            &USER_NAME_KEY_PARTS,
            &records_to_delete
                .iter()
                .map(|item| item.as_str())
                .collect::<Vec<&str>>(),
        )
        .await?;
        Self::remove_from_index_sorted_set(None, &USER_ID_KEY_PARTS, user_ids).await?;
        Ok(())
    }

    /// Finds all entries in the Sorted:Users:Name set that end with the given user_id
    async fn find_entries_by_user_id(user_id: &str) -> Result<Option<Vec<String>>, DynError> {
        // Use ZSCAN to iterate through the sorted set and find entries ending with :user_id
        use crate::db::get_redis_conn;
        
        let index_key = format!("Sorted:{}:{}", USER_NAME_KEY_PARTS[0], USER_NAME_KEY_PARTS[1]);
        let mut redis_conn = get_redis_conn().await?;
        let suffix = format!(":{}", user_id);
        let mut matching_entries = Vec::new();
        
        // Use ZSCAN to iterate through all entries
        let mut cursor = 0u64;
        loop {
            let (new_cursor, entries): (u64, Vec<String>) = redis::cmd("ZSCAN")
                .arg(&index_key)
                .arg(cursor)
                .query_async(&mut redis_conn)
                .await?;
            
            // ZSCAN returns alternating member/score pairs, we only want members (even indices)
            for (i, entry) in entries.iter().enumerate() {
                if i % 2 == 0 && entry.ends_with(&suffix) {
                    matching_entries.push(entry.clone());
                }
            }
            
            cursor = new_cursor;
            if cursor == 0 {
                break;
            }
        }
        
        if matching_entries.is_empty() {
            Ok(None)
        } else {
            Ok(Some(matching_entries))
        }
    }

    /// Deletes all sorted set entries for a given user_id
    /// This is used when completely removing a user from the system
    pub async fn delete_from_index(user_id: &str) -> Result<(), DynError> {
        Self::delete_existing_records(&[user_id]).await
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::{
//         models::{
//             traits::Collection,
//             user::{UserDetails, UserSearch},
//         },
//         types::DynError,
//         RedisOps,
//         _service::NexusApi,
//     };
//     use chrono::Utc;
//     use pubky_app_specs::PubkyId;

//     #[tokio_shared_rt::test(shared)]
//     async fn test_put_to_index_no_duplicates() -> Result<(), DynError> {
//         NexusApi::builder().init_stack().await;
//         // Test that the `put_to_index` method does not add duplicate records to the index
//         // when called with the same `UserDetails` multiple times.

//         // Create a `UserDetails` object
//         let user_id = "operrr8wsbpr3ue9d4qj41ge1kcc6r7fdiy6o3ugjrrhi4y77rdo";
//         let user_name = "Test User Duplicate";
//         let user_details = UserDetails {
//             id: PubkyId::try_from(user_id).expect("valid pubky id"),
//             name: user_name.to_string(),
//             bio: None,
//             status: None,
//             links: None,
//             image: None,
//             indexed_at: Utc::now().timestamp_millis(),
//         };

//         user_details.put_to_graph().await?;
//         user_details
//             .put_index_json(vec![user_id].as_slice(), None, None)
//             .await?;

//         // Call `put_to_index` with the same `UserDetails` object
//         UserSearch::put_to_index(&[&user_details]).await?;

//         // Check that the index contains only one record for the user
//         let search_result = UserSearch::get_by_name(&user_name, None, None).await?;
//         assert_eq!(search_result.unwrap().0, vec![user_id.to_string()]);

//         let new_user_name = "Some Other User Name";
//         let new_user_details = UserDetails {
//             id: PubkyId::try_from(user_id).expect("valid pubky id"),
//             name: new_user_name.to_string(),
//             bio: None,
//             status: None,
//             links: None,
//             image: None,
//             indexed_at: Utc::now().timestamp_millis(),
//         };

//         // Call `put_to_index` with new user details
//         UserSearch::put_to_index(&[&new_user_details]).await?;

//         // Check the previous record is deleted
//         // Check that the index contains only one record for the user
//         let search_result = UserSearch::get_by_name(&user_name, None, None).await?;
//         assert_eq!(search_result.is_none(), true);

//         Ok(())
//     }
//}
