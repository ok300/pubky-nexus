use serde::{Deserialize, Serialize};

/// Configuration for connecting to Neo4j database.
///
/// All values should be provided via configuration file or environment.
/// Default values are provided only for local development convenience.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Neo4JConfig {
    #[serde(default = "default_neo4j_uri")]
    pub uri: String,
    #[serde(default = "default_neo4j_user")]
    pub user: String,
    pub password: String,
}

fn default_neo4j_uri() -> String {
    String::from("bolt://localhost:7687")
}

fn default_neo4j_user() -> String {
    String::from("neo4j")
}

impl Default for Neo4JConfig {
    /// Default configuration for local development only.
    ///
    /// **Warning**: The default password is insecure and should never be used in production.
    /// Always provide credentials via configuration file in non-development environments.
    fn default() -> Self {
        Self {
            uri: default_neo4j_uri(),
            user: default_neo4j_user(),
            // Default password for local development only - must be overridden in production
            password: String::from("12345678"),
        }
    }
}
