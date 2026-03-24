use crate::types::{GraphQLEdge, GraphQLTransactionsResponse};
use thiserror::Error;
use tracing::debug;

#[derive(Debug, Error)]
pub enum GraphQLError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("GraphQL error: {0}")]
    Query(String),
    #[error("failed to parse response: {0}")]
    Parse(String),
}

/// Client for Arweave GraphQL queries via ar.io gateways.
#[derive(Debug, Clone)]
pub struct GraphQLClient {
    http: reqwest::Client,
    gateway: String,
}

/// Filter for GraphQL tag-based queries.
#[derive(Debug, Clone)]
pub struct TagFilter {
    pub name: String,
    pub values: Vec<String>,
}

impl TagFilter {
    pub fn new(name: impl Into<String>, values: Vec<String>) -> Self {
        Self {
            name: name.into(),
            values,
        }
    }

    pub fn single(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            values: vec![value.into()],
        }
    }
}

impl GraphQLClient {
    pub fn new(gateway: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            gateway: gateway.into(),
        }
    }

    /// Query transactions by tags with pagination.
    /// Returns a page of edges and whether there are more pages.
    pub async fn query_transactions(
        &self,
        tag_filters: &[TagFilter],
        first: usize,
        after: Option<&str>,
    ) -> Result<(Vec<GraphQLEdge>, bool), GraphQLError> {
        let query = build_transactions_query(tag_filters, first, after);

        debug!(
            gateway = %self.gateway,
            first = first,
            "querying Arweave transactions"
        );

        let url = format!("{}/graphql", self.gateway);
        let body = serde_json::json!({ "query": query });

        let response = self.http.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(GraphQLError::Query(text));
        }

        let gql_response: GraphQLTransactionsResponse = response
            .json()
            .await
            .map_err(|e| GraphQLError::Parse(e.to_string()))?;

        let has_next = gql_response.data.transactions.page_info.has_next_page;
        let edges = gql_response.data.transactions.edges;

        debug!(results = edges.len(), has_next, "received transaction results");

        Ok((edges, has_next))
    }

    /// Query all transactions matching the given tags, handling pagination automatically.
    pub async fn query_all_transactions(
        &self,
        tag_filters: &[TagFilter],
    ) -> Result<Vec<GraphQLEdge>, GraphQLError> {
        let mut all_edges = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let (edges, has_next) = self
                .query_transactions(tag_filters, 100, cursor.as_deref())
                .await?;

            if let Some(last) = edges.last() {
                cursor = Some(last.cursor.clone());
            }

            all_edges.extend(edges);

            if !has_next {
                break;
            }
        }

        Ok(all_edges)
    }

    /// Fetch raw transaction data by ID.
    pub async fn fetch_transaction_data(&self, tx_id: &str) -> Result<Vec<u8>, GraphQLError> {
        let url = format!("{}/raw/{}", self.gateway, tx_id);
        let response = self.http.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(GraphQLError::Query(format!(
                "failed to fetch tx {}: {}",
                tx_id,
                response.status()
            )));
        }

        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }
}

/// Build a GraphQL query string for transaction filtering.
fn build_transactions_query(
    tag_filters: &[TagFilter],
    first: usize,
    after: Option<&str>,
) -> String {
    let tags_clause = if tag_filters.is_empty() {
        String::new()
    } else {
        let filters: Vec<String> = tag_filters
            .iter()
            .map(|f| {
                let values = f
                    .values
                    .iter()
                    .map(|v| format!("\"{}\"", v.replace('"', "\\\"")))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{{ name: \"{}\", values: [{}] }}",
                    f.name.replace('"', "\\\""),
                    values
                )
            })
            .collect();
        format!("tags: [{}]", filters.join(", "))
    };

    let after_clause = match after {
        Some(cursor) => format!(", after: \"{}\"", cursor.replace('"', "\\\"")),
        None => String::new(),
    };

    format!(
        r#"{{
  transactions(
    {tags}
    first: {first}
    sort: HEIGHT_DESC
    {after}
  ) {{
    pageInfo {{
      hasNextPage
    }}
    edges {{
      cursor
      node {{
        id
        tags {{
          name
          value
        }}
        block {{
          height
          timestamp
        }}
        data {{
          size
          type
        }}
        owner {{
          address
        }}
      }}
    }}
  }}
}}"#,
        tags = tags_clause,
        first = first,
        after = after_clause,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_query_with_tags() {
        let filters = vec![
            TagFilter::single("App-Name", "UnlimitedContext"),
            TagFilter::single("UC-User-Id", "user_123"),
        ];
        let query = build_transactions_query(&filters, 100, None);
        assert!(query.contains("App-Name"));
        assert!(query.contains("UnlimitedContext"));
        assert!(query.contains("UC-User-Id"));
        assert!(query.contains("first: 100"));
    }

    #[test]
    fn test_build_query_with_pagination() {
        let filters = vec![TagFilter::single("App-Name", "UnlimitedContext")];
        let query = build_transactions_query(&filters, 50, Some("cursor_abc"));
        assert!(query.contains("after: \"cursor_abc\""));
        assert!(query.contains("first: 50"));
    }
}
