use crate::db::get_redis_conn;
use crate::types::DynError;
use deadpool_redis::redis::{AsyncCommands, Script};

/// Adds elements to a Redis list, avoiding duplicates.
///
/// This function appends elements to the specified Redis list only if they don't already exist.
/// This prevents duplicate entries when re-indexing or retrying operations. If the list doesn't exist,
/// it creates a new list.
///
/// # Arguments
///
/// * `prefix` - A string slice representing the prefix for the Redis keys.
/// * `key` - A string slice representing the key under which the list is stored.
/// * `values` - A slice of string slices representing the elements to be added to the list.
///
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn put(prefix: &str, key: &str, values: &[&str]) -> Result<(), DynError> {
    if values.is_empty() {
        return Ok(());
    }
    let index_key = format!("{prefix}:{key}");
    let mut redis_conn = get_redis_conn().await?;

    // Use a Lua script to atomically check for duplicates and add only new values
    // This prevents duplicate entries during re-indexing or retries
    let script = Script::new(
        r"
        local key = KEYS[1]
        local added = 0
        for i, value in ipairs(ARGV) do
            local exists = redis.call('LPOS', key, value)
            if not exists then
                redis.call('RPUSH', key, value)
                added = added + 1
            end
        end
        return added
        "
    );

    let _: i32 = script.key(&index_key).arg(values).invoke_async(&mut *redis_conn).await?;
    Ok(())
}

/// Retrieves a range of elements from a Redis list.
///
/// This function retrieves elements from a specified Redis list within a given range.
/// The range is defined by `skip` and `limit` parameters.
///
/// # Arguments
///
/// * `prefix` - A string slice representing the prefix for the Redis keys.
/// * `key` - A string slice representing the key under which the list is stored.
/// * `skip` - The number of elements to skip from the beginning of the list.
/// * `limit` - The number of elements to retrieve from the list after the skip.
///
/// # Returns
///
/// Returns a vector of strings containing the retrieved elements.
///
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn get_range(
    prefix: &str,
    key: &str,
    skip: Option<usize>,
    limit: Option<usize>,
) -> Result<Option<Vec<String>>, DynError> {
    let mut redis_conn = get_redis_conn().await?;

    let index_key = format!("{prefix}:{key}");
    let skip = skip.unwrap_or(0);
    let limit = limit.unwrap_or(usize::MAX);

    let start = skip as isize;
    let end = start + (limit as isize) - 1;
    let result: Vec<String> = redis_conn.lrange(index_key, start, end).await?;
    match result.len() {
        0 => Ok(None),
        _ => Ok(Some(result)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StackConfig;
    use crate::StackManager;

    #[tokio_shared_rt::test(shared)]
    async fn test_put_prevents_duplicates() -> Result<(), DynError> {
        StackManager::setup("unit-lists-test", &StackConfig::default()).await?;

        let prefix = "test_lists";
        let key = "duplicate_test";
        let test_values = &["value1", "value2", "value3"];

        // Clean up any existing test data
        let mut redis_conn = get_redis_conn().await?;
        let index_key = format!("{prefix}:{key}");
        let _: () = redis_conn.del(&index_key).await?;

        // First insertion: add the values
        put(prefix, key, test_values).await?;

        // Verify values were added
        let result = get_range(prefix, key, None, None).await?;
        assert!(result.is_some());
        let values = result.unwrap();
        assert_eq!(values.len(), 3);
        assert_eq!(values, vec!["value1", "value2", "value3"]);

        // Second insertion: try to add the same values again
        put(prefix, key, test_values).await?;

        // Verify no duplicates were created
        let result = get_range(prefix, key, None, None).await?;
        assert!(result.is_some());
        let values = result.unwrap();
        assert_eq!(values.len(), 3, "Duplicates were not prevented!");
        assert_eq!(values, vec!["value1", "value2", "value3"]);

        // Third insertion: add mix of existing and new values
        let mixed_values = &["value2", "value4", "value1", "value5"];
        put(prefix, key, mixed_values).await?;

        // Verify only new values were added
        let result = get_range(prefix, key, None, None).await?;
        assert!(result.is_some());
        let values = result.unwrap();
        assert_eq!(values.len(), 5, "Mixed insertion failed!");
        assert_eq!(
            values,
            vec!["value1", "value2", "value3", "value4", "value5"]
        );

        // Clean up
        let _: () = redis_conn.del(&index_key).await?;

        Ok(())
    }

    #[tokio_shared_rt::test(shared)]
    async fn test_put_empty_values() -> Result<(), DynError> {
        StackManager::setup("unit-lists-test", &StackConfig::default()).await?;

        let prefix = "test_lists";
        let key = "empty_test";
        let empty_values: &[&str] = &[];

        // Should not error on empty values
        put(prefix, key, empty_values).await?;

        // Verify nothing was added
        let result = get_range(prefix, key, None, None).await?;
        assert!(result.is_none());

        Ok(())
    }

    #[tokio_shared_rt::test(shared)]
    async fn test_get_range_with_skip_and_limit() -> Result<(), DynError> {
        StackManager::setup("unit-lists-test", &StackConfig::default()).await?;

        let prefix = "test_lists";
        let key = "range_test";
        let test_values = &["a", "b", "c", "d", "e", "f"];

        // Clean up any existing test data
        let mut redis_conn = get_redis_conn().await?;
        let index_key = format!("{prefix}:{key}");
        let _: () = redis_conn.del(&index_key).await?;

        // Add values
        put(prefix, key, test_values).await?;

        // Test skip
        let result = get_range(prefix, key, Some(2), None).await?;
        assert!(result.is_some());
        let values = result.unwrap();
        assert_eq!(values, vec!["c", "d", "e", "f"]);

        // Test limit
        let result = get_range(prefix, key, None, Some(3)).await?;
        assert!(result.is_some());
        let values = result.unwrap();
        assert_eq!(values, vec!["a", "b", "c"]);

        // Test skip + limit
        let result = get_range(prefix, key, Some(1), Some(3)).await?;
        assert!(result.is_some());
        let values = result.unwrap();
        assert_eq!(values, vec!["b", "c", "d"]);

        // Clean up
        let _: () = redis_conn.del(&index_key).await?;

        Ok(())
    }
}
