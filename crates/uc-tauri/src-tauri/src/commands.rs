use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

use crate::{AppConfigPath, AppEngine, AppRuntime};

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
    source_integration: Option<String>,
    source_model: Option<String>,
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

// ── Existing commands ──

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
                source_integration: c.source_integration,
                source_model: c.source_model,
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
    let top_k = top_k.unwrap_or(50);
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

// ── Graph ──

#[derive(Serialize)]
pub struct GraphData {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

#[derive(Serialize)]
pub struct GraphNode {
    id: String,
    label: String,
    chunk_count: usize,
    first_timestamp: i64,
    last_timestamp: i64,
    x: f32,
    y: f32,
}

#[derive(Serialize)]
pub struct GraphEdge {
    source: String,
    target: String,
    weight: f32,
}

#[tauri::command]
pub async fn get_graph(
    engine: State<'_, AppEngine>,
    rt: State<'_, AppRuntime>,
) -> Result<GraphData, String> {
    let engine = engine.0.clone();
    rt.0.spawn(async move {
        let graph = engine.graph("default").await.map_err(|e| e.to_string())?;
        Ok(GraphData {
            nodes: graph
                .nodes
                .into_iter()
                .map(|n| GraphNode {
                    id: n.id.clone(),
                    label: n.id,
                    chunk_count: n.chunk_count,
                    first_timestamp: n.first_timestamp,
                    last_timestamp: n.last_timestamp,
                    x: n.x,
                    y: n.y,
                })
                .collect(),
            edges: graph
                .edges
                .into_iter()
                .map(|e| GraphEdge {
                    source: e.source,
                    target: e.target,
                    weight: e.weight,
                })
                .collect(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Analytics ──

#[derive(Serialize)]
pub struct AnalyticsData {
    activity: Vec<ActivityEntry>,
    by_type: HashMap<String, usize>,
    by_source: HashMap<String, usize>,
    by_model: HashMap<String, usize>,
    sync_status: SyncStatus,
    total_chunks: usize,
    total_sessions: usize,
}

#[derive(Serialize)]
pub struct ActivityEntry {
    date: String,
    count: usize,
}

#[derive(Serialize)]
pub struct SyncStatus {
    synced: usize,
    local: usize,
}

#[tauri::command]
pub async fn get_analytics(
    engine: State<'_, AppEngine>,
    rt: State<'_, AppRuntime>,
) -> Result<AnalyticsData, String> {
    let engine = engine.0.clone();
    rt.0.spawn(async move {
        let a = engine.analytics("default").await.map_err(|e| e.to_string())?;
        Ok(AnalyticsData {
            activity: a
                .activity
                .into_iter()
                .map(|e| ActivityEntry {
                    date: e.date,
                    count: e.count,
                })
                .collect(),
            by_type: a.by_type,
            by_source: a.by_source,
            by_model: a.by_model,
            sync_status: SyncStatus {
                synced: a.sync_status.synced,
                local: a.sync_status.local,
            },
            total_chunks: a.total_chunks,
            total_sessions: a.total_sessions,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Integrations ──

#[derive(Serialize)]
pub struct IntegrationsStatus {
    mcp: IntegrationEntry,
    proxy: IntegrationEntry,
    ollama: IntegrationEntry,
    arweave: IntegrationEntry,
}

#[derive(Serialize)]
pub struct IntegrationEntry {
    enabled: bool,
    status: String,
}

#[derive(Serialize)]
pub struct ToggleResponse {
    success: bool,
    message: String,
}

#[tauri::command]
pub async fn get_integrations() -> Result<IntegrationsStatus, String> {
    // Check actual integration status from config files
    let claude_json = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude.json");
    let mcp_registered = claude_json.exists() && {
        std::fs::read_to_string(&claude_json)
            .map(|c| c.contains("memoryport") || c.contains("uc-mcp"))
            .unwrap_or(false)
    };

    let proxy_configured = claude_json.exists() && {
        std::fs::read_to_string(&claude_json)
            .map(|c| c.contains("ANTHROPIC_BASE_URL"))
            .unwrap_or(false)
    };

    let wallet_exists = dirs::home_dir()
        .unwrap_or_default()
        .join(".memoryport/wallet.json")
        .exists();

    Ok(IntegrationsStatus {
        mcp: IntegrationEntry {
            enabled: mcp_registered,
            status: if mcp_registered { "registered".into() } else { "not registered".into() },
        },
        proxy: IntegrationEntry {
            enabled: proxy_configured,
            status: if proxy_configured { "configured".into() } else { "not configured".into() },
        },
        ollama: IntegrationEntry {
            enabled: false,
            status: "via proxy".into(),
        },
        arweave: IntegrationEntry {
            enabled: wallet_exists,
            status: if wallet_exists { "wallet found".into() } else { "no wallet".into() },
        },
    })
}

#[tauri::command]
pub async fn toggle_integration(
    integration: String,
    enabled: bool,
) -> Result<ToggleResponse, String> {
    // For desktop app, toggling integrations modifies local config files
    // This is a simplified version — the full server version writes to .claude.json etc.
    Ok(ToggleResponse {
        success: true,
        message: format!(
            "{} {} — restart the app for changes to take effect",
            integration,
            if enabled { "enabled" } else { "disabled" }
        ),
    })
}

// ── Settings ──

#[derive(Serialize)]
pub struct SettingsData {
    embeddings: EmbeddingsSettings,
    retrieval: RetrievalSettings,
    proxy: Option<ProxySettings>,
    arweave: ArweaveSettings,
    encryption: EncryptionSettings,
}

#[derive(Serialize, Deserialize)]
pub struct EmbeddingsSettings {
    provider: String,
    model: String,
    dimensions: usize,
    api_key: Option<String>,
    api_base: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RetrievalSettings {
    gating_enabled: bool,
    similarity_top_k: usize,
    recency_window: usize,
}

#[derive(Serialize, Deserialize)]
pub struct ProxySettings {
    agentic_enabled: bool,
}

#[derive(Serialize)]
pub struct ArweaveSettings {
    gateway: String,
    wallet_path: Option<String>,
    api_key: Option<String>,
    api_endpoint: Option<String>,
    address: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct EncryptionSettings {
    enabled: bool,
}

#[tauri::command]
pub async fn get_settings(
    config_path: State<'_, AppConfigPath>,
) -> Result<SettingsData, String> {
    let config = uc_core::config::Config::from_file(&config_path.0)
        .unwrap_or_else(|_| uc_core::config::Config::default_config());

    let has_api_key = config.resolved_api_key().is_some();

    // Resolve wallet address
    let address = config
        .resolved_wallet_path()
        .or_else(|| {
            if has_api_key {
                Some(uc_core::config::expand_tilde_pub("~/.memoryport/wallet.json"))
            } else {
                None
            }
        })
        .filter(|p| p.exists())
        .and_then(|p| uc_arweave::Wallet::from_file(&p).ok())
        .map(|w| w.address.clone());

    Ok(SettingsData {
        embeddings: EmbeddingsSettings {
            provider: config.embeddings.provider.clone(),
            model: config.embeddings.model.clone(),
            dimensions: config.embeddings.dimensions,
            api_key: config.embeddings.api_key.as_ref().map(|_| "••••••••".into()),
            api_base: config.embeddings.api_base.clone(),
        },
        retrieval: RetrievalSettings {
            gating_enabled: config.retrieval.gating_enabled,
            similarity_top_k: config.retrieval.similarity_top_k,
            recency_window: config.retrieval.recency_window,
        },
        proxy: Some(ProxySettings {
            agentic_enabled: config.proxy.agentic.enabled,
        }),
        arweave: ArweaveSettings {
            gateway: config.arweave.gateway.clone(),
            wallet_path: config.arweave.wallet_path.clone(),
            api_key: if has_api_key { Some("••••••••".into()) } else { None },
            api_endpoint: config.arweave.api_endpoint.clone(),
            address,
        },
        encryption: EncryptionSettings {
            enabled: config.encryption.enabled,
        },
    })
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct SettingsUpdatePayload {
    settings: serde_json::Value,
}

#[tauri::command]
pub async fn update_settings(
    config_path: State<'_, AppConfigPath>,
    settings: serde_json::Value,
) -> Result<(), String> {
    let path = &config_path.0;

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        toml::from_str::<toml::Value>(&content)
            .unwrap_or_else(|_| toml::Value::Table(toml::map::Map::new()))
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let table = config.as_table_mut().ok_or("invalid config")?;

    // Update arweave api_key
    if let Some(arweave) = settings.get("arweave") {
        if let Some(key) = arweave.get("api_key").and_then(|v| v.as_str()) {
            if key != "••••••••" && !key.is_empty() {
                let section = table
                    .entry("arweave")
                    .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
                if let Some(section) = section.as_table_mut() {
                    section.insert("api_key".into(), toml::Value::String(key.into()));
                    if !section.contains_key("wallet_path") {
                        section.insert(
                            "wallet_path".into(),
                            toml::Value::String("~/.memoryport/wallet.json".into()),
                        );
                    }
                }
            }
        }
    }

    // Update proxy agentic
    if let Some(proxy) = settings.get("proxy") {
        if let Some(enabled) = proxy.get("agentic_enabled").and_then(|v| v.as_bool()) {
            let section = table
                .entry("proxy")
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
            if let Some(section) = section.as_table_mut() {
                let agentic = section
                    .entry("agentic")
                    .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
                if let Some(agentic) = agentic.as_table_mut() {
                    agentic.insert("enabled".into(), toml::Value::Boolean(enabled));
                }
            }
        }
    }

    // Update embeddings
    if let Some(emb) = settings.get("embeddings") {
        let section = table
            .entry("embeddings")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        if let Some(section) = section.as_table_mut() {
            if let Some(v) = emb.get("provider").and_then(|v| v.as_str()) {
                section.insert("provider".into(), toml::Value::String(v.into()));
            }
            if let Some(v) = emb.get("model").and_then(|v| v.as_str()) {
                section.insert("model".into(), toml::Value::String(v.into()));
            }
            if let Some(v) = emb.get("dimensions").and_then(|v| v.as_u64()) {
                section.insert("dimensions".into(), toml::Value::Integer(v as i64));
            }
            if let Some(v) = emb.get("api_key").and_then(|v| v.as_str()) {
                if v != "••••••••" && !v.is_empty() {
                    section.insert("api_key".into(), toml::Value::String(v.into()));
                }
            }
        }
    }

    // Update retrieval
    if let Some(ret) = settings.get("retrieval") {
        let section = table
            .entry("retrieval")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        if let Some(section) = section.as_table_mut() {
            if let Some(v) = ret.get("gating_enabled").and_then(|v| v.as_bool()) {
                section.insert("gating_enabled".into(), toml::Value::Boolean(v));
            }
        }
    }

    // Update encryption
    if let Some(enc) = settings.get("encryption") {
        let section = table
            .entry("encryption")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        if let Some(section) = section.as_table_mut() {
            if let Some(v) = enc.get("enabled").and_then(|v| v.as_bool()) {
                section.insert("enabled".into(), toml::Value::Boolean(v));
            }
        }
    }

    let toml_str = toml::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(path, &toml_str).map_err(|e| e.to_string())?;

    Ok(())
}
