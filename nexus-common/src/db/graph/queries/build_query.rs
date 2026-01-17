
use crate::{
    models::post::StreamSource,
    types::{Pagination, StreamSorting},
};
use neo4rs::{query, Query};
use pubky_app_specs::PubkyAppPostKind;

/// Builds a Cypher query for retrieving a stream of posts based on various criteria.
pub(super) struct PostStreamQueryBuilder {
    source: StreamSource,
    sorting: StreamSorting,
    tags: Option<Vec<String>>,
    pagination: Pagination,
    kind: Option<PubkyAppPostKind>,
    cypher: String,
    where_clause_applied: bool,
}

impl PostStreamQueryBuilder {
    /// Creates a new `PostStreamQueryBuilder`.
    pub(super) fn new(
        source: StreamSource,
        sorting: StreamSorting,
        tags: Option<Vec<String>>,
        pagination: Pagination,
        kind: Option<PubkyAppPostKind>,
    ) -> Self {
        Self {
            source,
            sorting,
            tags,
            pagination,
            kind,
            cypher: String::new(),
            where_clause_applied: false,
        }
    }

    /// Builds the final `Query` object.
    pub(super) fn build(mut self) -> Query {
        self.build_match_clause();
        self.build_where_clause();
        self.build_with_clause();
        self.build_order_clause();
        self.build_return_clause();
        self.build_pagination_clause();
        self.build_query_with_params()
    }

    /// Appends a condition to the Cypher query.
    fn append_condition(&mut self, condition: &str) {
        if self.where_clause_applied {
            self.cypher.push_str(&format!("AND {condition}\n"));
        } else {
            self.cypher.push_str(&format!("WHERE {condition}\n"));
            self.where_clause_applied = true;
        }
    }

    /// Builds the `MATCH` clause of the Cypher query.
    fn build_match_clause(&mut self) {
        if self.source.get_observer().is_some() {
            self.cypher
                .push_str("MATCH (observer:User {id: $observer_id})\n");
        }

        self.cypher
            .push_str("MATCH (p:Post)<-[:AUTHORED]-(author:User)\n");

        if let Some(query) = match self.source {
            StreamSource::Following { .. } => Some("MATCH (observer)-[:FOLLOWS]->(author)\n"),
            StreamSource::Followers { .. } => Some("MATCH (observer)<-[:FOLLOWS]-(author)\n"),
            StreamSource::Friends { .. } => {
                Some("MATCH (observer)-[:FOLLOWS]->(author)-[:FOLLOWS]->(observer)\n")
            }
            StreamSource::Bookmarks { .. } => Some("MATCH (observer)-[:BOOKMARKED]->(p)\n"),
            _ => None,
        } {
            self.cypher.push_str(query);
        }

        if self.tags.is_some() {
            self.cypher.push_str("MATCH (User)-[tag:TAGGED]->(p)\n");
        }
    }

    /// Builds the `WHERE` clause of the Cypher query.
    fn build_where_clause(&mut self) {
        if self.tags.is_some() {
            self.append_condition("tag.label IN $labels");
        }

        if self.source.get_author().is_some() {
            self.append_condition("author.id = $author_id");
        }

        if self.kind.is_some() {
            self.append_condition("p.kind = $kind");
        }

        self.append_condition("NOT ( (p)-[:REPLIED]->(:Post) )");

        if self.sorting == StreamSorting::Timeline {
            if self.pagination.start.is_some() {
                self.append_condition("p.indexed_at <= $start");
            }
            if self.pagination.end.is_some() {
                self.append_condition("p.indexed_at >= $end");
            }
        }
    }

    /// Builds the `WITH` clause of the Cypher query.
    fn build_with_clause(&mut self) {
        self.cypher.push_str("WITH DISTINCT p, author\n");
    }

    /// Builds the `ORDER BY` clause of the Cypher query.
    fn build_order_clause(&mut self) {
        let order_clause = match self.sorting {
            StreamSorting::Timeline => "ORDER BY p.indexed_at DESC".to_string(),
            StreamSorting::TotalEngagement => {
                self.cypher.push_str(
                    "
                    OPTIONAL MATCH (p)<-[tag:TAGGED]-(:User)
                    OPTIONAL MATCH (p)<-[reply:REPLIED]-(:Post)
                    OPTIONAL MATCH (p)<-[repost:REPOSTED]-(:Post)
                    WITH p, author,
                        COUNT(DISTINCT tag) AS tags_count,
                        COUNT(DISTINCT reply) AS replies_count,
                        COUNT(DISTINCT repost) AS reposts_count,
                        (COUNT(DISTINCT tag) + COUNT(DISTINCT reply) + COUNT(DISTINCT repost)) AS total_engagement
                    ",
                );

                self.where_clause_applied = false;

                if self.pagination.start.is_some() {
                    self.append_condition("total_engagement <= $start");
                }
                if self.pagination.end.is_some() {
                    self.append_condition("total_engagement >= $end");
                }

                "ORDER BY total_engagement DESC".to_string()
            }
        };
        self.cypher.push_str(&format!("{order_clause}\n"));
    }

    /// Builds the `RETURN` clause of the Cypher query.
    fn build_return_clause(&mut self) {
        self.cypher
            .push_str("RETURN author.id AS author_id, p.id AS post_id, p.indexed_at AS indexed_at\n");
    }

    /// Builds the pagination clause of the Cypher query.
    fn build_pagination_clause(&mut self) {
        if let Some(skip) = self.pagination.skip {
            self.cypher.push_str(&format!("SKIP {skip}\n"));
        }
        if let Some(limit) = self.pagination.limit {
            self.cypher.push_str(&format!("LIMIT {limit}\n"));
        }
    }

    /// Builds the final `Query` object with parameters.
    fn build_query_with_params(self) -> Query {
        let mut query = query(&self.cypher);

        if let Some(observer_id) = self.source.get_observer() {
            query = query.param("observer_id", observer_id.to_string());
        }
        if let Some(labels) = self.tags {
            query = query.param("labels", labels);
        }
        if let Some(author_id) = self.source.get_author() {
            query = query.param("author_id", author_id.to_string());
        }
        if let Some(post_kind) = self.kind {
            query = query.param("kind", post_kind.to_string());
        }
        if let Some(start_interval) = self.pagination.start {
            query = query.param("start", start_interval);
        }
        if let Some(end_interval) = self.pagination.end {
            query = query.param("end", end_interval);
        }

        query
    }
}
