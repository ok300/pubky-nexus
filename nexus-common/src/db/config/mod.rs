use serde::{Deserialize, Serialize};
use std::fmt::Debug;

mod neo4j;
pub use neo4j::Neo4JConfig;

fn default_redis_uri() -> String {
    String::from("redis://localhost:6379")
}

/// Configuration for database connections.
///
/// Default values are provided only for local development convenience.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct DatabaseConfig {
    #[serde(default = "default_redis_uri")]
    pub redis: String,
    pub neo4j: Neo4JConfig,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            redis: default_redis_uri(),
            neo4j: Neo4JConfig::default(),
        }
    }
}
