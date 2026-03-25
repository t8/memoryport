use serde::Serialize;
use tauri::State;

use crate::{AppEngine, AppRuntime};

#[derive(Serialize)]
pub struct StatusResponse {
    pending_chunks: usize,
    indexed_chunks: usize,
    index_path: String,
    embedding_model: String,
    embedding_dimensions: usize,
}

#[derive(Serialize)]
pub struct SessionInfo {
    session_id: String,
    chunk_count: usize,
    first_timestamp: i64,
    last_timestamp: i64,
}

#[derive(Serialize)]
pub struct SessionChunk {
    chunk_id: String,
    role: Option<String>,
    content: String,
    timestamp: i64,
}

#[derive(Serialize)]
pub struct SearchResult {
    chunk_id: String,
    session_id: String,
    chunk_type: String,
    role: Option<String>,
    score: f32,
    timestamp: i64,
    content: String,
    arweave_tx_id: String,
}

#[tauri::command]
pub async fn get_status(
    engine: State<'_, AppEngine>,
    rt: State<'_, AppRuntime>,
) -> Result<StatusResponse, String> {
    let engine = engine.0.clone();
    rt.0.spawn(async move {
        let s = engine.status().await.map_err(|e| e.to_string())?;
        Ok(StatusResponse {
            pending_chunks: s.pending_chunks,
            indexed_chunks: s.indexed_chunks,
            index_path: s.index_path,
            embedding_model: s.embedding_model,
            embedding_dimensions: s.embedding_dimensions,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_sessions(
    engine: State<'_, AppEngine>,
    rt: State<'_, AppRuntime>,
) -> Result<Vec<SessionInfo>, String> {
    let engine = engine.0.clone();
    rt.0.spawn(async move {
        let sessions = engine.list_sessions("default").await.map_err(|e| e.to_string())?;
        Ok(sessions
            .into_iter()
            .map(|s| SessionInfo {
                session_id: s.session_id,
                chunk_count: s.chunk_count,
                first_timestamp: s.first_timestamp,
                last_timestamp: s.last_timestamp,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_session(
    engine: State<'_, AppEngine>,
    rt: State<'_, AppRuntime>,
    session_id: String,
) -> Result<Vec<SessionChunk>, String> {
    let engine = engine.0.clone();
    rt.0.spawn(async move {
        let chunks = engine
            .get_session("default", &session_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(chunks
            .into_iter()
            .map(|c| SessionChunk {
                chunk_id: c.chunk_id,
                role: c.role.map(|r| r.as_str().to_string()),
                content: c.content,
                timestamp: c.timestamp,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn retrieve(
    engine: State<'_, AppEngine>,
    rt: State<'_, AppRuntime>,
    query: String,
    top_k: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    let engine = engine.0.clone();
    let top_k = top_k.unwrap_or(10);
    rt.0.spawn(async move {
        let results = engine
            .retrieve(&query, "default", None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(results
            .into_iter()
            .take(top_k)
            .map(|r| SearchResult {
                chunk_id: r.chunk_id,
                session_id: r.session_id,
                chunk_type: r.chunk_type.as_str().to_string(),
                role: r.role.map(|r| r.as_str().to_string()),
                score: r.score,
                timestamp: r.timestamp,
                content: r.content,
                arweave_tx_id: r.arweave_tx_id,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn store_text(
    engine: State<'_, AppEngine>,
    rt: State<'_, AppRuntime>,
    text: String,
    session_id: Option<String>,
) -> Result<usize, String> {
    let engine = engine.0.clone();
    rt.0.spawn(async move {
        let params = uc_core::models::StoreParams {
            user_id: "default".into(),
            session_id: session_id.unwrap_or_else(|| "default".into()),
            chunk_type: uc_core::models::ChunkType::Conversation,
            role: Some(uc_core::models::Role::User),
            source_integration: Some("desktop".into()),
            source_model: None,
        };
        let ids = engine.store(&text, params).await.map_err(|e| e.to_string())?;
        let _ = engine.flush().await;
        Ok(ids.len())
    })
    .await
    .map_err(|e| e.to_string())?
}
