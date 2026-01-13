use neo4rs::{query, BoltType, Query};
use std::collections::HashMap;

pub struct QueryBuilder {
    matches: Vec<String>,
    conditions: Vec<String>,
    with_clauses: Vec<String>,
    with_conditions: Vec<String>,
    return_clause: Option<String>,
    order_by: Option<String>,
    skip: Option<usize>,
    limit: Option<usize>,
    params: HashMap<String, BoltType>,
}

impl QueryBuilder {
    pub fn new() -> Self {
        QueryBuilder {
            matches: Vec::new(),
            conditions: Vec::new(),
            with_clauses: Vec::new(),
            with_conditions: Vec::new(),
            return_clause: None,
            order_by: None,
            skip: None,
            limit: None,
            params: HashMap::new(),
        }
    }

    pub fn add_match(&mut self, match_clause: &str) -> &mut Self {
        self.matches.push(match_clause.to_string());
        self
    }

    pub fn add_condition(&mut self, condition: &str) -> &mut Self {
        self.conditions.push(condition.to_string());
        self
    }

    pub fn add_with_condition(&mut self, condition: &str) -> &mut Self {
        self.with_conditions.push(condition.to_string());
        self
    }

    pub fn add_with(&mut self, with_clause: &str) -> &mut Self {
        self.with_clauses.push(with_clause.to_string());
        self
    }

    pub fn set_return(&mut self, return_clause: &str) -> &mut Self {
        self.return_clause = Some(return_clause.to_string());
        self
    }

    pub fn set_order_by(&mut self, order_by: &str) -> &mut Self {
        self.order_by = Some(order_by.to_string());
        self
    }

    pub fn set_skip(&mut self, skip: usize) -> &mut Self {
        self.skip = Some(skip);
        self
    }

    pub fn set_limit(&mut self, limit: usize) -> &mut Self {
        self.limit = Some(limit);
        self
    }

    pub fn add_param(&mut self, key: &str, value: impl Into<BoltType>) -> &mut Self {
        self.params.insert(key.to_string(), value.into());
        self
    }

    fn generate_cypher_string(&self) -> String {
        let mut cypher = String::new();

        for m in &self.matches {
            cypher.push_str(&format!("{}\n", m));
        }

        if !self.conditions.is_empty() {
            cypher.push_str("WHERE ");
            cypher.push_str(&self.conditions.join(" AND "));
            cypher.push('\n');
        }

        for w in &self.with_clauses {
            cypher.push_str(&format!("{}\n", w));
        }

        if !self.with_conditions.is_empty() {
            cypher.push_str("WHERE ");
            cypher.push_str(&self.with_conditions.join(" AND "));
            cypher.push('\n');
        }

        if let Some(ret) = &self.return_clause {
            cypher.push_str(&format!("{}\n", ret));
        }

        if let Some(order) = &self.order_by {
            cypher.push_str(&format!("{}\n", order));
        }

        if let Some(skip) = self.skip {
            cypher.push_str(&format!("SKIP {}\n", skip));
        }

        if let Some(limit) = self.limit {
            cypher.push_str(&format!("LIMIT {}\n", limit));
        }

        cypher
    }

    pub fn build(&self) -> Query {
        let cypher = self.generate_cypher_string();
        let mut final_query = query(&cypher);
        for (key, value) in &self.params {
            final_query = final_query.param(key, value.clone());
        }

        final_query
    }

    #[cfg(test)]
    pub(crate) fn cypher(&self) -> String {
        self.generate_cypher_string()
    }

    #[cfg(test)]
    pub(crate) fn params(&self) -> &HashMap<String, BoltType> {
        &self.params
    }
}
