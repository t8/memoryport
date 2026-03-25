use serde::{Deserialize, Serialize};

// -- Requests --

#[derive(Debug, Deserialize)]
pub struct StoreRequest {
    pub text: String,
    #[serde(default = "default_session")]
    pub session_id: String,
    #[serde(default = "default_chunk_type")]
    pub chunk_type: String,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub query: String,
    pub session_id: Option<String>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

#[derive(Debug, Deserialize)]
pub struct RetrieveRequest {
    pub query: String,
    pub session_id: Option<String>,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub email: Option<String>,
}

// -- Responses --

#[derive(Debug, Serialize)]
pub struct StoreResponse {
    pub chunk_ids: Vec<String>,
    pub chunks_stored: usize,
}

#[derive(Debug, Serialize)]
pub struct QueryResponse {
    pub context: String,
    pub token_count: u32,
    pub chunks_included: usize,
}

#[derive(Debug, Serialize)]
pub struct RetrieveResponse {
    pub results: Vec<RetrieveResult>,
}

#[derive(Debug, Serialize)]
pub struct RetrieveResult {
    pub chunk_id: String,
    pub session_id: String,
    pub chunk_type: String,
    pub role: Option<String>,
    pub score: f32,
    pub timestamp: i64,
    pub content: String,
    pub arweave_tx_id: String,
}

#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub chunk_count: usize,
    pub first_timestamp: i64,
    pub last_timestamp: i64,
}

#[derive(Debug, Serialize)]
pub struct SessionDetailResponse {
    pub session_id: String,
    pub chunks: Vec<SessionChunk>,
}

#[derive(Debug, Serialize)]
pub struct SessionChunk {
    pub chunk_id: String,
    pub role: Option<String>,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub pending_chunks: usize,
    pub indexed_chunks: usize,
    pub index_path: String,
    pub embedding_model: String,
    pub embedding_dimensions: usize,
}

#[derive(Debug, Serialize)]
pub struct CreateUserResponse {
    pub user_id: String,
    pub api_key: String,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ReadyResponse {
    pub status: &'static str,
    pub checks: ReadyChecks,
}

#[derive(Debug, Serialize)]
pub struct ReadyChecks {
    pub database: bool,
}

// -- Defaults --

fn default_session() -> String {
    "default".into()
}

fn default_chunk_type() -> String {
    "conversation".into()
}

fn default_max_tokens() -> u32 {
    50_000
}

fn default_top_k() -> usize {
    10
}
