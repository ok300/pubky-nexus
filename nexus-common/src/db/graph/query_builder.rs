use neo4rs::{BoltType, Query};
use std::collections::HashMap;

#[derive(Default)]
pub struct QueryBuilder<'a> {
    matches: Vec<String>,
    conditions: Vec<String>,
    with: Vec<String>,
    return_clause: Option<String>,
    order_by: Option<String>,
    skip: Option<usize>,
    limit: Option<usize>,
    params: HashMap<&'a str, BoltType>,
}

impl<'a> QueryBuilder<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn match_clause(mut self, clause: &str) -> Self {
        self.matches.push(clause.to_string());
        self
    }

    pub fn where_clause(mut self, condition: &str) -> Self {
        self.conditions.push(condition.to_string());
        self
    }

    pub fn with_clause(mut self, clause: &str) -> Self {
        self.with.push(clause.to_string());
        self
    }

    pub fn return_clause(mut self, clause: &str) -> Self {
        self.return_clause = Some(clause.to_string());
        self
    }

    pub fn order_by(mut self, order: &str) -> Self {
        self.order_by = Some(order.to_string());
        self
    }

    pub fn skip(mut self, skip: usize) -> Self {
        self.skip = Some(skip);
        self
    }


    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn param<T: Into<BoltType>>(mut self, key: &'a str, value: T) -> Self {
        self.params.insert(key, value.into());
        self
    }

    pub fn build(self) -> Query {
        let mut cypher = String::new();

        for m in &self.matches {
            cypher.push_str(m);
            cypher.push('\n');
        }

        if !self.conditions.is_empty() {
            cypher.push_str("WHERE ");
            cypher.push_str(&self.conditions.join(" AND "));
            cypher.push('\n');
        }

        for w in &self.with {
            cypher.push_str(&format!("WITH {}\n", w));
        }

        if let Some(return_clause) = &self.return_clause {
            cypher.push_str(&format!("RETURN {}\n", return_clause));
        }

        if let Some(order) = &self.order_by {
            cypher.push_str(&format!("ORDER BY {}\n", order));
        }

        if let Some(skip) = self.skip {
            cypher.push_str(&format!("SKIP {}\n", skip));
        }

        if let Some(limit) = self.limit {
            cypher.push_str(&format!("LIMIT {}\n", limit));
        }

        let mut query = neo4rs::query(&cypher);
        for (key, value) in self.params {
            query = query.param(key, value);
        }
        query
    }
}
