use serde::{Deserialize, Serialize};

/// An Arweave transaction tag (key-value pair).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub value: String,
}

impl Tag {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    /// Total byte size of this tag (name + value in UTF-8).
    pub fn byte_size(&self) -> usize {
        self.name.len() + self.value.len()
    }
}

/// A signed ANS-104 data item ready for upload.
#[derive(Debug, Clone)]
pub struct SignedDataItem {
    /// The data item ID (base64url of SHA-256 of signature).
    pub id: String,
    /// The raw binary bytes of the signed data item.
    pub bytes: Vec<u8>,
    /// The owner address (base64url of SHA-256 of public key modulus).
    pub owner_address: String,
}

/// Response from ar.io Turbo after uploading a data item.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurboUploadResponse {
    pub id: String,
    pub owner: Option<String>,
    pub data_caches: Option<Vec<String>>,
    pub fast_finality_indexes: Option<Vec<String>>,
    pub deadline_height: Option<u64>,
    pub timestamp: Option<u64>,
}

/// A single edge in a GraphQL paginated response.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLEdge {
    pub cursor: String,
    pub node: GraphQLNode,
}

/// A transaction node from GraphQL.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLNode {
    pub id: String,
    pub tags: Vec<GraphQLTag>,
    pub block: Option<GraphQLBlock>,
    pub data: Option<GraphQLData>,
    pub owner: Option<GraphQLOwner>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLTag {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLBlock {
    pub height: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLData {
    pub size: Option<String>,
    #[serde(rename = "type")]
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLOwner {
    pub address: String,
}

/// Paginated GraphQL response wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLTransactionsResponse {
    pub data: GraphQLTransactionsData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLTransactionsData {
    pub transactions: GraphQLTransactionsPage,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLTransactionsPage {
    pub page_info: GraphQLPageInfo,
    pub edges: Vec<GraphQLEdge>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLPageInfo {
    pub has_next_page: bool,
}
