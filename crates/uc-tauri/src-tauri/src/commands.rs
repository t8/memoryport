use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;

use crate::services::ServiceHealthResponse;
use crate::{get_engine, AppConfigPath, AppEngine, AppRuntime, AppServices};

// ── Response types ──

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

// ── Data commands ──

#[tauri::command]
pub async fn get_status(
    engine: State<'_, AppEngine>,
    rt: State<'_, AppRuntime>,
) -> Result<StatusResponse, String> {
    let engine = get_engine(&engine).await?;
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
    let engine = get_engine(&engine).await?;
    rt.0.spawn(async move {
        let sessions = engine
            .list_sessions("default")
            .await
            .map_err(|e| e.to_string())?;
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
    let engine = get_engine(&engine).await?;
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
    let engine = get_engine(&engine).await?;
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
    let engine = get_engine(&engine).await?;
    rt.0.spawn(async move {
        let params = uc_core::models::StoreParams {
            user_id: "default".into(),
            session_id: session_id.unwrap_or_else(|| "default".into()),
            chunk_type: uc_core::models::ChunkType::Conversation,
            role: Some(uc_core::models::Role::User),
            source_integration: Some("desktop".into()),
            source_model: None,
        };
        let ids = engine
            .store(&text, params)
            .await
            .map_err(|e| e.to_string())?;
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
    let engine = get_engine(&engine).await?;
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
    let engine = get_engine(&engine).await?;
    rt.0.spawn(async move {
        let a = engine
            .analytics("default")
            .await
            .map_err(|e| e.to_string())?;
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
    let claude_json = dirs::home_dir().unwrap_or_default().join(".claude.json");
    let mcp_registered = claude_json.exists()
        && std::fs::read_to_string(&claude_json)
            .map(|c| c.contains("memoryport") || c.contains("uc-mcp"))
            .unwrap_or(false);

    let proxy_configured = claude_json.exists()
        && std::fs::read_to_string(&claude_json)
            .map(|c| c.contains("ANTHROPIC_BASE_URL"))
            .unwrap_or(false);

    let wallet_exists = dirs::home_dir()
        .unwrap_or_default()
        .join(".memoryport/wallet.json")
        .exists();

    Ok(IntegrationsStatus {
        mcp: IntegrationEntry {
            enabled: mcp_registered,
            status: if mcp_registered {
                "registered".into()
            } else {
                "not registered".into()
            },
        },
        proxy: IntegrationEntry {
            enabled: proxy_configured,
            status: if proxy_configured {
                "configured".into()
            } else {
                "not configured".into()
            },
        },
        ollama: IntegrationEntry {
            enabled: false,
            status: "via proxy".into(),
        },
        arweave: IntegrationEntry {
            enabled: wallet_exists,
            status: if wallet_exists {
                "wallet found".into()
            } else {
                "no wallet".into()
            },
        },
    })
}

#[tauri::command]
pub async fn toggle_integration(
    integration: String,
    enabled: bool,
) -> Result<ToggleResponse, String> {
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
    enabled: bool,
    api_endpoint: Option<String>,
    address: Option<String>,
    storage_used_bytes: Option<u64>,
    storage_limit_bytes: Option<u64>,
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
            api_key: config
                .embeddings
                .api_key
                .as_ref()
                .map(|_| "••••••••".into()),
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
            api_key: if has_api_key {
                Some("••••••••".into())
            } else {
                None
            },
            enabled: config.arweave.enabled,
            api_endpoint: config.arweave.api_endpoint.clone(),
            address,
            storage_used_bytes: None, // Populated when engine has AccountClient cache
            storage_limit_bytes: None,
        },
        encryption: EncryptionSettings {
            enabled: config.encryption.enabled,
        },
    })
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

    if let Some(arweave) = settings.get("arweave") {
        let section = table
            .entry("arweave")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        if let Some(section) = section.as_table_mut() {
            if let Some(key) = arweave.get("api_key").and_then(|v| v.as_str()) {
                if key != "••••••••" && !key.is_empty() {
                    section.insert("api_key".into(), toml::Value::String(key.into()));
                    if !section.contains_key("wallet_path") {
                        section.insert(
                            "wallet_path".into(),
                            toml::Value::String("~/.memoryport/wallet.json".into()),
                        );
                    }
                }
            }
            if let Some(enabled) = arweave.get("enabled").and_then(|v| v.as_bool()) {
                section.insert("enabled".into(), toml::Value::Boolean(enabled));
            }
        }
    }

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

// ── Setup + lifecycle commands ──

#[tauri::command]
pub async fn check_config_exists(
    config_path: State<'_, AppConfigPath>,
) -> Result<bool, String> {
    Ok(config_path.0.exists())
}

#[derive(Deserialize)]
pub struct SetupConfig {
    pub provider: String,        // "openai" or "ollama"
    pub model: String,           // e.g. "text-embedding-3-small"
    pub dimensions: usize,       // e.g. 1536
    pub api_key: Option<String>, // OpenAI API key
    pub uc_api_key: Option<String>, // Memoryport Pro key
}

#[tauri::command]
pub async fn write_initial_config(
    config_path: State<'_, AppConfigPath>,
    config: SetupConfig,
) -> Result<(), String> {
    let uc_dir = config_path.0.parent().unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(uc_dir.join("index")).map_err(|e| e.to_string())?;

    let mut toml_content = format!(
        "[arweave]\ngateway = \"https://arweave.net\"\nturbo_endpoint = \"https://upload.ardrive.io\"\n"
    );

    if let Some(ref key) = config.uc_api_key {
        toml_content.push_str(&format!("api_key = \"{key}\"\n"));
        toml_content.push_str(&format!(
            "wallet_path = \"{}/wallet.json\"\n",
            uc_dir.display()
        ));
    }

    toml_content.push_str(&format!(
        "\n[index]\npath = \"{}/index\"\nembedding_dimensions = {}\n",
        uc_dir.display(),
        config.dimensions
    ));

    toml_content.push_str(&format!(
        "\n[embeddings]\nprovider = \"{}\"\nmodel = \"{}\"\ndimensions = {}\n",
        config.provider, config.model, config.dimensions
    ));

    if let Some(ref key) = config.api_key {
        toml_content.push_str(&format!("api_key = \"{key}\"\n"));
    }

    toml_content.push_str(
        "\n[retrieval]\ngating_enabled = true\nmax_context_tokens = 50000\nrecency_window = 20\nsimilarity_top_k = 50\n"
    );

    toml_content.push_str("\n[proxy]\nlisten = \"127.0.0.1:9191\"\n");

    std::fs::write(&config_path.0, &toml_content).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn init_engine(
    engine_state: State<'_, AppEngine>,
    config_path: State<'_, AppConfigPath>,
    rt: State<'_, AppRuntime>,
) -> Result<(), String> {
    let path = config_path.0.clone();
    let new_engine = rt
        .0
        .spawn(async move {
            let config =
                uc_core::config::Config::from_file(&path).map_err(|e| e.to_string())?;
            uc_core::Engine::new(config)
                .await
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;

    let mut guard = engine_state.0.write().await;
    *guard = Some(Arc::new(new_engine));

    Ok(())
}

#[tauri::command]
pub async fn get_service_health(
    services: State<'_, AppServices>,
) -> Result<ServiceHealthResponse, String> {
    let guard = services.0.read().await;
    match &*guard {
        Some(svc) => Ok(svc.health().await),
        None => Err("Service manager not initialized".into()),
    }
}

#[tauri::command]
pub async fn start_services(
    services: State<'_, AppServices>,
) -> Result<(), String> {
    let guard = services.0.read().await;
    if let Some(ref svc) = *guard {
        svc.start_all().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn stop_services(
    services: State<'_, AppServices>,
) -> Result<(), String> {
    let guard = services.0.read().await;
    if let Some(ref svc) = *guard {
        svc.stop_all().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn restart_service(
    services: State<'_, AppServices>,
    _service: String,
) -> Result<(), String> {
    // For now, restart all — individual service restart comes with sidecar support
    let guard = services.0.read().await;
    if let Some(ref svc) = *guard {
        svc.stop_all().await;
        svc.start_all().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn check_ollama_installed() -> Result<bool, String> {
    Ok(which::which("ollama").is_ok())
}

#[tauri::command]
pub async fn install_ollama() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        // On Windows, open the download page
        Ok("open:https://ollama.com/download".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        // On macOS/Linux, run the installer
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg("curl -fsSL https://ollama.com/install.sh | sh")
            .output()
            .await
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            Ok("installed".into())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Install failed: {stderr}"))
        }
    }
}

#[tauri::command]
pub async fn pull_ollama_model(model: String) -> Result<(), String> {
    let output = tokio::process::Command::new("ollama")
        .args(["pull", &model])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Pull failed: {stderr}"))
    }
}

#[tauri::command]
pub async fn register_mcp() -> Result<(), String> {
    let claude_json = dirs::home_dir()
        .ok_or("no home dir")?
        .join(".claude.json");

    let mut data: serde_json::Value = if claude_json.exists() {
        let content = std::fs::read_to_string(&claude_json).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Find uc-mcp binary
    let mcp_path = which::which("uc-mcp")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".memoryport/bin/uc-mcp")
                .to_string_lossy()
                .to_string()
        });

    let config_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".memoryport/uc.toml")
        .to_string_lossy()
        .to_string();

    let mcp_entry = serde_json::json!({
        "command": mcp_path,
        "args": ["--config", config_path],
        "type": "stdio"
    });

    data.as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert(serde_json::json!({}))
        .as_object_mut()
        .unwrap()
        .insert("memoryport".into(), mcp_entry);

    let content = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    std::fs::write(&claude_json, content).map_err(|e| e.to_string())?;

    // Also register in Cursor
    let cursor_json = dirs::home_dir()
        .unwrap_or_default()
        .join(".cursor/mcp.json");
    if let Some(parent) = cursor_json.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut cursor_data: serde_json::Value = if cursor_json.exists() {
        let content = std::fs::read_to_string(&cursor_json).unwrap_or("{}".into());
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let mcp_entry = serde_json::json!({
        "command": mcp_path,
        "args": ["--config", config_path],
        "type": "stdio"
    });

    cursor_data
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert(serde_json::json!({}))
        .as_object_mut()
        .unwrap()
        .insert("memoryport".into(), mcp_entry);

    let content = serde_json::to_string_pretty(&cursor_data).map_err(|e| e.to_string())?;
    std::fs::write(&cursor_json, content).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn register_proxy() -> Result<(), String> {
    let claude_json = dirs::home_dir()
        .ok_or("no home dir")?
        .join(".claude.json");

    let proxy_url = "http://127.0.0.1:9191";

    let mut data: serde_json::Value = if claude_json.exists() {
        let content = std::fs::read_to_string(&claude_json).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let env = data
        .as_object_mut()
        .unwrap()
        .entry("env")
        .or_insert(serde_json::json!({}));

    if let Some(env_obj) = env.as_object_mut() {
        // Save original ANTHROPIC_BASE_URL before overwriting
        if let Some(original) = env_obj.get("ANTHROPIC_BASE_URL") {
            if original.as_str() != Some(proxy_url) {
                env_obj.insert(
                    "_MEMORYPORT_ORIGINAL_BASE_URL".into(),
                    original.clone(),
                );
            }
        }
        env_obj.insert(
            "ANTHROPIC_BASE_URL".into(),
            serde_json::json!(proxy_url),
        );
    }

    let content = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    std::fs::write(&claude_json, content).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn unregister_proxy() -> Result<(), String> {
    let claude_json = dirs::home_dir()
        .ok_or("no home dir")?
        .join(".claude.json");

    if !claude_json.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&claude_json).map_err(|e| e.to_string())?;
    let mut data: serde_json::Value =
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

    if let Some(env) = data.get_mut("env").and_then(|e| e.as_object_mut()) {
        // Restore original if we saved one, otherwise remove entirely
        if let Some(original) = env.remove("_MEMORYPORT_ORIGINAL_BASE_URL") {
            env.insert("ANTHROPIC_BASE_URL".into(), original);
        } else {
            env.remove("ANTHROPIC_BASE_URL");
        }
    }

    let content = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    std::fs::write(&claude_json, content).map_err(|e| e.to_string())?;

    Ok(())
}

// ── Data recovery ──

#[derive(Serialize)]
pub struct RebuildResult {
    pub chunks_restored: usize,
}

#[tauri::command]
pub async fn rebuild_from_arweave(
    engine: State<'_, AppEngine>,
    rt: State<'_, AppRuntime>,
) -> Result<RebuildResult, String> {
    let engine = get_engine(&engine).await?;
    rt.0.spawn(async move {
        let progress = engine
            .rebuild_index("default")
            .await
            .map_err(|e| e.to_string())?;
        Ok(RebuildResult {
            chunks_restored: progress.chunks_indexed,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
