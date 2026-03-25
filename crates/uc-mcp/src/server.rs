use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler};
use rmcp::service::RequestContext;
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;
use uc_core::Engine;

#[derive(Clone)]
pub struct UcMcpServer {
    engine: Arc<Engine>,
    default_user_id: String,
    tool_router: ToolRouter<Self>,
}

impl UcMcpServer {
    pub fn new(engine: Arc<Engine>, default_user_id: String) -> Self {
        Self {
            engine,
            default_user_id,
            tool_router: Self::tool_router(),
        }
    }
}

// -- Tool parameter types --

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StoreParams {
    /// The text content to store.
    pub text: String,
    /// User identifier.
    pub user_id: Option<String>,
    /// Session identifier.
    pub session_id: Option<String>,
    /// Content type: conversation, document, or knowledge.
    pub chunk_type: Option<String>,
    /// Role: user, assistant, or system.
    pub role: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryParams {
    /// The search query text.
    pub query: String,
    /// User identifier.
    pub user_id: Option<String>,
    /// Active session ID for recency and session affinity.
    pub session_id: Option<String>,
    /// Maximum tokens for assembled context.
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RetrieveParams {
    /// The search query text.
    pub query: String,
    /// User identifier.
    pub user_id: Option<String>,
    /// Active session ID.
    pub session_id: Option<String>,
    /// Number of results to return.
    pub top_k: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSessionParams {
    /// Session identifier to retrieve.
    pub session_id: String,
    /// User identifier.
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSessionsParams {
    /// User identifier.
    pub user_id: Option<String>,
}

// -- Tool implementations --

#[tool_router(router = tool_router)]
impl UcMcpServer {
    #[tool(description = "Store text content to Arweave permanent storage and local vector index")]
    pub async fn uc_store(&self, Parameters(params): Parameters<StoreParams>) -> String {
        let user_id = params.user_id.unwrap_or_else(|| self.default_user_id.clone());
        let session_id = params.session_id.unwrap_or_else(|| "default".into());
        let chunk_type = params
            .chunk_type
            .as_deref()
            .unwrap_or("conversation")
            .parse()
            .unwrap_or(uc_core::models::ChunkType::Conversation);
        let role = params.role.as_deref().and_then(|r| r.parse().ok());

        let store_params = uc_core::models::StoreParams {
            user_id,
            session_id,
            chunk_type,
            role,
        };

        match self.engine.store(&params.text, store_params).await {
            Ok(ids) => {
                let _ = self.engine.flush().await;
                format!("Stored {} chunk(s)", ids.len())
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(description = "Search stored context using vector similarity, reranking, and context assembly")]
    pub async fn uc_query(&self, Parameters(params): Parameters<QueryParams>) -> String {
        let user_id = params.user_id.as_deref().unwrap_or(&self.default_user_id);
        let max_tokens = params.max_tokens.unwrap_or(50_000);

        match self
            .engine
            .query(&params.query, user_id, params.session_id.as_deref(), max_tokens)
            .await
        {
            Ok(ctx) if ctx.chunks_included == 0 => "No matching context found.".into(),
            Ok(ctx) => ctx.formatted,
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(description = "Retrieve raw ranked results without context assembly")]
    pub async fn uc_retrieve(&self, Parameters(params): Parameters<RetrieveParams>) -> String {
        let user_id = params.user_id.as_deref().unwrap_or(&self.default_user_id);
        let top_k = params.top_k.unwrap_or(10);

        match self
            .engine
            .retrieve(&params.query, user_id, params.session_id.as_deref())
            .await
        {
            Ok(results) => {
                let output: Vec<serde_json::Value> = results
                    .iter()
                    .take(top_k)
                    .map(|r| serde_json::json!({
                        "chunk_id": r.chunk_id,
                        "session_id": r.session_id,
                        "chunk_type": r.chunk_type.as_str(),
                        "score": r.score,
                        "timestamp": r.timestamp,
                        "content": r.content,
                    }))
                    .collect();
                serde_json::to_string_pretty(&output).unwrap_or_default()
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(description = "Get full conversation history for a specific session")]
    pub async fn uc_get_session(&self, Parameters(params): Parameters<GetSessionParams>) -> String {
        let user_id = params.user_id.as_deref().unwrap_or(&self.default_user_id);

        match self.engine.get_session(user_id, &params.session_id).await {
            Ok(results) => {
                let output: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| serde_json::json!({
                        "role": r.role.map(|r| r.as_str()),
                        "content": r.content,
                        "timestamp": r.timestamp,
                    }))
                    .collect();
                serde_json::to_string_pretty(&output).unwrap_or_default()
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(description = "List all stored sessions with metadata")]
    pub async fn uc_list_sessions(&self, Parameters(params): Parameters<ListSessionsParams>) -> String {
        let user_id = params.user_id.as_deref().unwrap_or(&self.default_user_id);

        match self.engine.list_sessions(user_id).await {
            Ok(sessions) => {
                let output: Vec<serde_json::Value> = sessions
                    .iter()
                    .map(|s| serde_json::json!({
                        "session_id": s.session_id,
                        "chunk_count": s.chunk_count,
                        "first_timestamp": s.first_timestamp,
                        "last_timestamp": s.last_timestamp,
                    }))
                    .collect();
                serde_json::to_string_pretty(&output).unwrap_or_default()
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(description = "Get system status: index size, pending writes, embedding model")]
    pub async fn uc_status(&self) -> String {
        match self.engine.status().await {
            Ok(s) => format!(
                "Pending: {}\nIndexed: {}\nPath: {}\nModel: {} ({}d)",
                s.pending_chunks, s.indexed_chunks, s.index_path,
                s.embedding_model, s.embedding_dimensions,
            ),
            Err(e) => format!("Error: {e}"),
        }
    }
}

// -- ServerHandler with tool routing and resources --

#[tool_handler(router = self.tool_router)]
impl ServerHandler for UcMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        );
        info.instructions = Some(
            "Unlimited Context — persistent LLM memory on Arweave. \
             Use uc_store to save context, uc_query to retrieve it."
                .into(),
        );
        info
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        async {
            Ok(ListResourcesResult {
                resources: vec![Annotated::new(
                    RawResource::new("uc://context/auto", "Auto Context")
                        .with_description("Recent context from the active session")
                        .with_mime_type("text/xml"),
                    None,
                )],
                ..Default::default()
            })
        }
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, McpError>> + Send + '_ {
        async {
            Ok(ListResourceTemplatesResult {
                resource_templates: vec![Annotated::new(
                    RawResourceTemplate::new("uc://sessions/{id}", "Session Transcript")
                        .with_description("Full conversation history for a session")
                        .with_mime_type("application/json"),
                    None,
                )],
                ..Default::default()
            })
        }
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        async move {
            let uri = &request.uri;

            if uri == "uc://context/auto" {
                let sessions = self
                    .engine
                    .list_sessions(&self.default_user_id)
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                let content = if let Some(latest) = sessions.first() {
                    let ctx = self
                        .engine
                        .query("", &self.default_user_id, Some(&latest.session_id), 10_000)
                        .await
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    ctx.formatted
                } else {
                    "<unlimited_context/>".into()
                };

                Ok(ReadResourceResult::new(vec![ResourceContents::text(content, uri.as_str())]))
            } else if let Some(session_id) = uri.strip_prefix("uc://sessions/") {
                let results = self
                    .engine
                    .get_session(&self.default_user_id, session_id)
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                let output: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| serde_json::json!({
                        "role": r.role.map(|r| r.as_str()),
                        "content": r.content,
                        "timestamp": r.timestamp,
                    }))
                    .collect();

                Ok(ReadResourceResult::new(vec![ResourceContents::text(
                        serde_json::to_string_pretty(&output).unwrap_or_default(),
                        uri.as_str(),
                    )]))
            } else {
                Err(McpError::resource_not_found(
                    "unknown resource",
                    Some(serde_json::json!({ "uri": uri })),
                ))
            }
        }
    }
}
