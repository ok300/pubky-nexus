use neo4rs::query;
use nexus_common::{
    db::{fetch_all_rows_from_graph, get_neo4j_graph},
    types::DynError,
    ApiConfig,
};
use nexus_webapi::{api_context::ApiContextBuilder, NexusApiBuilder};
use std::time::{Duration, Instant};
use tracing::info;

/// Neo4j operations helper
pub struct N4jOps;

impl N4jOps {
    /// Initialize the stack (Neo4j and Redis connections)
    async fn init_stack() -> Result<(), DynError> {
        let api_context = ApiContextBuilder::from_default_config_dir()
            .api_config(ApiConfig::default())
            .try_build()
            .await?;
        NexusApiBuilder(api_context).init_stack().await?;
        Ok(())
    }

    /// Check connectivity to Neo4j
    pub async fn check() -> Result<(), DynError> {
        Self::init_stack().await?;

        let graph = get_neo4j_graph()?;
        let graph = graph.lock().await;

        match graph.execute(query("RETURN 1")).await {
            Ok(_) => {
                println!("Neo4j connection check: SUCCESS");
                info!("Neo4j connectivity check succeeded");
                Ok(())
            }
            Err(e) => {
                println!("Neo4j connection check: FAILURE");
                println!("Error: {e}");
                Err(format!("Neo4j connection failed: {e}").into())
            }
        }
    }

    /// Perform user lookups to warmup Neo4j's page cache
    pub async fn user_warmup() -> Result<(), DynError> {
        Self::init_stack().await?;

        println!("Performing user lookups for Neo4j page cache warmup...");

        // Get a sample of users to warmup the page cache.
        // Limit of 100 provides good coverage for typical graph sizes while
        // keeping warmup time reasonable.
        let warmup_query = query(
            "MATCH (u:User) 
             WITH u LIMIT 100 
             RETURN u.id AS user_id",
        );

        let rows = fetch_all_rows_from_graph(warmup_query).await?;
        let user_count = rows.len();

        // Perform lookups on each user
        for row in rows {
            if let Ok(user_id) = row.get::<String>("user_id") {
                let _ = Self::lookup_user(&user_id).await;
            }
        }

        println!("User warmup complete. Processed {user_count} users.");
        Ok(())
    }

    /// Perform follow lookups to warmup Neo4j's page cache
    pub async fn follows_warmup() -> Result<(), DynError> {
        Self::init_stack().await?;

        println!("Performing follow lookups for Neo4j page cache warmup...");

        // Get a sample of users with followers to warmup the page cache.
        // Limit of 50 top followed users provides good coverage of the follower
        // relationships while keeping warmup time reasonable.
        let warmup_query = query(
            "MATCH (u:User)<-[:FOLLOWS]-(:User) 
             WITH u, COUNT(*) AS follower_count 
             ORDER BY follower_count DESC 
             LIMIT 50 
             RETURN u.id AS user_id",
        );

        let rows = fetch_all_rows_from_graph(warmup_query).await?;
        let user_count = rows.len();

        // Perform follow lookups on each user
        for row in rows {
            if let Ok(user_id) = row.get::<String>("user_id") {
                let _ = Self::lookup_followers(&user_id, 1).await;
            }
        }

        println!("Follows warmup complete. Processed {user_count} users.");
        Ok(())
    }

    /// Measure N-th degree fan-in performance
    pub async fn follows_n(degree: u8) -> Result<(), DynError> {
        Self::init_stack().await?;

        println!("Measuring {degree}-degree fan-in performance...\n");

        // Find representative users
        let representatives = Self::find_representative_users().await?;

        println!("Representative users found:");
        for (category, user_id, follower_count) in &representatives {
            println!("  {category}: {user_id} ({follower_count} followers)");
        }
        println!();

        // Perform measurements for each representative.
        // 1000 iterations provides statistically meaningful results for
        // benchmarking query performance while keeping total runtime manageable.
        const ITERATIONS: u32 = 1000;

        for (category, user_id, follower_count) in &representatives {
            let mut total_duration = Duration::ZERO;

            for _ in 0..ITERATIONS {
                let start = Instant::now();
                let _ = Self::lookup_followers(user_id, degree).await;
                total_duration += start.elapsed();
            }

            let avg_duration = total_duration / ITERATIONS;

            println!("{category} ({user_id}, {follower_count} followers):",);
            println!(
                "  Total: {:?}, Average: {:?} ({ITERATIONS} iterations)",
                total_duration, avg_duration
            );
        }

        Ok(())
    }

    /// Find representative users with varying follower counts
    async fn find_representative_users() -> Result<Vec<(String, String, i64)>, DynError> {
        // Query to get users ordered by follower count
        let query_str = "
            MATCH (u:User)
            OPTIONAL MATCH (u)<-[f:FOLLOWS]-(:User)
            WITH u, COUNT(f) AS follower_count
            ORDER BY follower_count DESC
            RETURN u.id AS user_id, follower_count
        ";

        let rows = fetch_all_rows_from_graph(query(query_str)).await?;

        if rows.is_empty() {
            return Err("No users found in the database".into());
        }

        let total_users = rows.len();
        let mut representatives = Vec::new();

        // Collect user_id and follower_count pairs
        let users: Vec<(String, i64)> = rows
            .into_iter()
            .filter_map(|row| {
                let user_id = row.get::<String>("user_id").ok()?;
                let follower_count = row.get::<i64>("follower_count").unwrap_or(0);
                Some((user_id, follower_count))
            })
            .collect();

        if users.is_empty() {
            return Err("No valid users found".into());
        }

        // Most followers (first)
        if let Some((user_id, count)) = users.first() {
            representatives.push(("Most followers".to_string(), user_id.clone(), *count));
        }

        // High followers (25th percentile from top)
        if total_users > 4 {
            let idx = total_users / 4;
            if let Some((user_id, count)) = users.get(idx) {
                representatives.push(("High followers".to_string(), user_id.clone(), *count));
            }
        }

        // Medium followers (50th percentile)
        if total_users > 2 {
            let idx = total_users / 2;
            if let Some((user_id, count)) = users.get(idx) {
                representatives.push(("Medium followers".to_string(), user_id.clone(), *count));
            }
        }

        // Low followers (75th percentile from top)
        if total_users > 4 {
            let idx = (total_users * 3) / 4;
            if let Some((user_id, count)) = users.get(idx) {
                representatives.push(("Low followers".to_string(), user_id.clone(), *count));
            }
        }

        // Lowest followers (last)
        if let Some((user_id, count)) = users.last() {
            // Add lowest followers if we don't have 5 representatives yet,
            // or if this user is different from the last added representative
            let should_add = representatives.len() < 5
                || representatives
                    .last()
                    .map(|(_, id, _)| id != user_id)
                    .unwrap_or(true);
            if should_add {
                representatives.push(("Lowest followers".to_string(), user_id.clone(), *count));
            }
        }

        Ok(representatives)
    }

    /// Lookup a user by ID
    async fn lookup_user(user_id: &str) -> Result<bool, DynError> {
        let q = query("MATCH (u:User {id: $user_id}) RETURN u.id AS id").param("user_id", user_id);

        let rows = fetch_all_rows_from_graph(q).await?;
        Ok(!rows.is_empty())
    }

    /// Lookup followers at the specified degree (1 = direct followers, 2 = followers of followers, etc.)
    ///
    /// Note: Variable-length path patterns like `[:FOLLOWS*1..N]` can be expensive for high degrees
    /// in large graphs. The degree is limited to 5 by the CLI commands to balance usefulness with
    /// performance. For production use with very large graphs, consider query timeouts.
    async fn lookup_followers(user_id: &str, degree: u8) -> Result<Vec<String>, DynError> {
        // Build the query based on degree
        let query_str = match degree {
            1 => "MATCH (u:User {id: $user_id})<-[:FOLLOWS]-(follower:User) 
                  RETURN COLLECT(follower.id) AS follower_ids"
                .to_string(),
            _ => format!(
                "MATCH (u:User {{id: $user_id}})<-[:FOLLOWS*1..{degree}]-(follower:User) 
                 RETURN COLLECT(DISTINCT follower.id) AS follower_ids"
            ),
        };

        let q = query(&query_str).param("user_id", user_id);
        let rows = fetch_all_rows_from_graph(q).await?;

        if let Some(row) = rows.into_iter().next() {
            let follower_ids: Vec<String> = row.get("follower_ids").unwrap_or_default();
            return Ok(follower_ids);
        }

        Ok(vec![])
    }
}
