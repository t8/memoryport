use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, warn};

pub struct ProxyState {
    pub engine: Arc<uc_core::Engine>,
    pub http: reqwest::Client,
    pub user_id: String,
    pub sessions: SessionManager,
    pub context_budget: u32,
    pub agentic_config: HotConfig,
    pub no_tool_models: Mutex<HashSet<String>>,
    /// Optional upstream override (from config or env). Used for Anthropic routing.
    pub anthropic_upstream: Option<String>,
}

/// Hot-reloadable config that re-reads from disk when the file changes.
pub struct HotConfig {
    config_path: std::path::PathBuf,
    cached: Mutex<HotConfigCache>,
}

struct HotConfigCache {
    agentic: uc_core::config::AgenticProxyConfig,
    mtime: Option<std::time::SystemTime>,
}

impl HotConfig {
    pub fn new(config_path: std::path::PathBuf, initial: uc_core::config::AgenticProxyConfig) -> Self {
        let mtime = std::fs::metadata(&config_path).ok().and_then(|m| m.modified().ok());
        Self {
            config_path,
            cached: Mutex::new(HotConfigCache { agentic: initial, mtime }),
        }
    }

    pub async fn agentic(&self) -> uc_core::config::AgenticProxyConfig {
        let current_mtime = std::fs::metadata(&self.config_path).ok().and_then(|m| m.modified().ok());
        let mut cache = self.cached.lock().await;

        if current_mtime != cache.mtime {
            // File changed — reload
            if let Ok(config) = uc_core::config::Config::from_file(&self.config_path) {
                debug!("hot-reloaded proxy config from disk");
                cache.agentic = config.proxy.agentic;
                cache.mtime = current_mtime;
            }
        }

        cache.agentic.clone()
    }
}

/// Manages session IDs per source. Creates a new session after 30 minutes of inactivity.
/// Persists to disk so sessions survive proxy restarts.
pub struct SessionManager {
    active: Mutex<HashMap<String, ActiveSession>>,
    inactivity_timeout_secs: u64,
    state_file: std::path::PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ActiveSession {
    session_id: String,
    last_activity_epoch: u64, // unix seconds (Instant doesn't serialize)
}

impl SessionManager {
    pub fn new(inactivity_timeout_secs: u64) -> Self {
        let state_file = dirs::home_dir()
            .unwrap_or_default()
            .join(".memoryport")
            .join("proxy-sessions.json");

        // Load persisted state
        let active = if let Ok(data) = std::fs::read_to_string(&state_file) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Self {
            active: Mutex::new(active),
            inactivity_timeout_secs,
            state_file,
        }
    }

    /// Get or create a session ID for a given source.
    /// Rotates to a new session after inactivity timeout.
    pub async fn get_session(&self, source: &str) -> String {
        let mut sessions = self.active.lock().await;
        let now_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(active) = sessions.get_mut(source) {
            if now_epoch - active.last_activity_epoch < self.inactivity_timeout_secs {
                active.last_activity_epoch = now_epoch;
                let sid = active.session_id.clone();
                self.persist(&sessions);
                return sid;
            }
        }

        // Create new session
        let session_id = format!(
            "{}-{}",
            source,
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        );
        sessions.insert(
            source.to_string(),
            ActiveSession {
                session_id: session_id.clone(),
                last_activity_epoch: now_epoch,
            },
        );
        self.persist(&sessions);
        session_id
    }

    fn persist(&self, sessions: &HashMap<String, ActiveSession>) {
        if let Ok(json) = serde_json::to_string(sessions) {
            let _ = std::fs::write(&self.state_file, json);
        }
    }
}

/// Detect upstream from model name in request.
fn detect_upstream(request: &serde_json::Value) -> &'static str {
    let model = request
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("");

    if model.starts_with("gpt-") || model.starts_with("o1") || model.starts_with("o3") || model.starts_with("text-") {
        "https://api.openai.com"
    } else if model.starts_with("llama")
        || model.starts_with("mistral")
        || model.starts_with("codellama")
        || model.starts_with("gemma")
        || model.starts_with("phi")
        || model.starts_with("qwen")
        || model.starts_with("deepseek")
        || model.starts_with("nomic")
        || model.contains(":")
    {
        // Ollama always runs on its default port — we don't move it
        "http://localhost:11434"
    } else {
        // Default to OpenAI
        "https://api.openai.com"
    }
}

/// POST /v1/chat/completions — OpenAI-compatible proxy with auto-routing.
/// Routes to OpenAI, Ollama, or any OpenAI-compatible endpoint based on model name.
pub async fn proxy_completions(
    State(state): State<Arc<ProxyState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, StatusCode> {
    let mut request: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
        warn!(error = %e, "failed to parse request body");
        StatusCode::BAD_REQUEST
    })?;

    // Detect upstream
    let upstream = detect_upstream(&request);

    // Extract last user message
    let last_user_msg = extract_last_user_text(&request);

    if last_user_msg.is_empty() {
        return forward_openai_raw(&state, &headers, &body, upstream).await;
    }

    // Agentic path: inject tools and let the LLM query memory iteratively
    let agentic = state.agentic_config.agentic().await;
    if agentic.enabled && !crate::agentic::is_disabled_by_header(&headers) {
        let model = request.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
        let is_no_tool_model = state.no_tool_models.lock().await.contains(&model);

        if !is_no_tool_model {
            let format = if upstream.contains("localhost") || upstream.contains("127.0.0.1") {
                crate::agentic::ApiFormat::Ollama
            } else {
                crate::agentic::ApiFormat::OpenAi
            };
            return crate::agentic::run_agentic_loop(
                &state, &headers, &mut request, format, upstream, "openai",
            )
            .await;
        }
    }

    // Fallback: single-shot context injection (original behavior)
    debug!(query_len = last_user_msg.len(), upstream = upstream, "processing OpenAI-format request");

    let clean_query = crate::anthropic::sanitize_query_pub(&last_user_msg);

    // Get current session ID so we can exclude it from context injection
    let current_session = state.sessions.get_session("openai").await;

    // Search for context (best-effort), excluding current session
    let context = match state.engine.search(&clean_query, &state.user_id, 20).await {
        Ok(ref results) => {
            let clean: Vec<_> = results
                .iter()
                .filter(|r| !crate::anthropic::is_system_prompt_leak_pub(&r.content))
                .filter(|r| r.session_id != current_session) // exclude current conversation
                .cloned()
                .collect();
            if clean.is_empty() {
                None
            } else {
                Some(uc_core::assembler::assemble_context(&clean, state.context_budget))
            }
        }
        Err(e) => {
            warn!(error = %e, "context retrieval failed");
            None
        }
    };

    // Inject context into last user message
    if let Some(ref ctx) = context {
        if ctx.chunks_included > 0 {
            let plain_context = crate::anthropic::format_plain_context(&ctx.formatted);
            let suffix = format!(
                "\n\nFor additional context, here is information from my previous conversations:\n\n{}\n\nPlease use the above context to answer my question.",
                plain_context
            );
            append_to_last_user_message(&mut request, &suffix);
        }
    }

    // Forward
    let modified_body = serde_json::to_vec(&request).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response = forward_openai_raw(&state, &headers, &modified_body, upstream).await?;

    // Store user message (async)
    let model = request.get("model").and_then(|m| m.as_str()).unwrap_or("unknown").to_string();
    {
        let engine = state.engine.clone();
        let user_id = state.user_id.clone();
        let session_id = state.sessions.get_session("openai").await;
        let msg = last_user_msg;
        tokio::spawn(async move {
            let msg = crate::anthropic::sanitize_for_storage_pub(&msg);
            if msg.len() < 10 { return; }
            let params = uc_core::models::StoreParams {
                user_id,
                session_id,
                chunk_type: uc_core::models::ChunkType::Conversation,
                role: Some(uc_core::models::Role::User),
                source_integration: Some("proxy".into()),
                source_model: Some(model),
            };
            let _ = engine.store(&msg, params).await;
            let _ = engine.flush().await;
        });
    }

    Ok(response)
}

fn extract_last_user_text(request: &serde_json::Value) -> String {
    let messages = match request.get("messages").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return String::new(),
    };
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        if let Some(s) = msg.get("content").and_then(|c| c.as_str()) {
            return s.to_string();
        }
    }
    String::new()
}

fn append_to_last_user_message(request: &mut serde_json::Value, suffix: &str) {
    let messages = match request.get_mut("messages").and_then(|m| m.as_array_mut()) {
        Some(m) => m,
        None => return,
    };
    for msg in messages.iter_mut().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        if let Some(s) = msg.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()) {
            msg["content"] = serde_json::Value::String(format!("{}{}", s, suffix));
            return;
        }
        break;
    }
}

async fn forward_openai_raw(
    state: &ProxyState,
    original_headers: &HeaderMap,
    body: &[u8],
    upstream: &str,
) -> Result<Response, StatusCode> {
    let url = format!("{}/v1/chat/completions", upstream);

    let mut req_builder = state
        .http
        .post(&url)
        .header("content-type", "application/json")
        .body(body.to_vec());

    // Forward auth headers
    for (name, value) in original_headers.iter() {
        let n = name.as_str();
        if n == "authorization" || n == "x-api-key" {
            req_builder = req_builder.header(name, value);
        }
    }

    let upstream_resp = req_builder.send().await.map_err(|e| {
        warn!(error = %e, upstream = upstream, "failed to forward to upstream");
        StatusCode::BAD_GATEWAY
    })?;

    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let resp_headers = upstream_resp.headers().clone();
    let body_bytes = upstream_resp.bytes().await.map_err(|e| {
        warn!(error = %e, "failed to read upstream response");
        StatusCode::BAD_GATEWAY
    })?;

    // Store assistant response (best-effort)
    if status.is_success() {
        if let Ok(resp_json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
            let assistant_text = resp_json
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            let assistant_text = crate::anthropic::sanitize_for_storage_pub(&assistant_text);
            if assistant_text.len() >= 10 {
                let engine = state.engine.clone();
                let user_id = state.user_id.clone();
                let session_id = state.sessions.get_session("openai").await;
                let model = resp_json
                    .get("model")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                tokio::spawn(async move {
                    let params = uc_core::models::StoreParams {
                        user_id,
                        session_id,
                        chunk_type: uc_core::models::ChunkType::Conversation,
                        role: Some(uc_core::models::Role::Assistant),
                        source_integration: Some("proxy".into()),
                        source_model: Some(model),
                    };
                    let _ = engine.store(&assistant_text, params).await;
                    let _ = engine.flush().await;
                });
            }
        }
    }

    let mut response = (status, body_bytes).into_response();
    if let Some(ct) = resp_headers.get("content-type") {
        response.headers_mut().insert("content-type", ct.clone());
    }
    Ok(response)
}

pub async fn health() -> &'static str {
    "ok"
}

/// Respond to Ollama's root check (GET /) so clients think we're Ollama.
pub async fn ollama_root() -> &'static str {
    "Ollama is running"
}

/// Get the port where real Ollama is running (always default, we don't move it).
fn get_ollama_port() -> &'static str {
    "11434"
}

/// Forward any Ollama native API request (/api/*) to real Ollama.
/// Captures conversations from /api/chat and /api/generate.
pub async fn forward_ollama_any(
    State(state): State<Arc<ProxyState>>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, StatusCode> {
    let ollama_port = get_ollama_port();
    let path = uri.path().to_string();
    let upstream = format!("http://127.0.0.1:{}{}", ollama_port, path);
    eprintln!("[memoryport-proxy] forwarding {} {} -> {}", method, path, upstream);

    // Extract user message + model from /api/chat or /api/generate before forwarding
    let (user_msg, model) = if (path == "/api/chat" || path == "/api/generate") && !body.is_empty() {
        if let Ok(req_json) = serde_json::from_slice::<serde_json::Value>(&body) {
            let model = req_json.get("model").and_then(|m| m.as_str()).unwrap_or("unknown").to_string();
            let user_msg = if path == "/api/chat" {
                // /api/chat: messages array
                req_json.get("messages")
                    .and_then(|m| m.as_array())
                    .and_then(|msgs| msgs.iter().rev().find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user")))
                    .and_then(|m| m.get("content").and_then(|c| c.as_str()))
                    .unwrap_or("")
                    .to_string()
            } else {
                // /api/generate: prompt field
                req_json.get("prompt").and_then(|p| p.as_str()).unwrap_or("").to_string()
            };
            (user_msg, model)
        } else {
            (String::new(), "unknown".into())
        }
    } else {
        (String::new(), "unknown".into())
    };

    // Detect if this is an Open WebUI internal request (title/tag generation, emoji, etc.)
    let is_internal_request = if let Ok(req_json) = serde_json::from_slice::<serde_json::Value>(&body) {
        req_json.get("messages")
            .and_then(|m| m.as_array())
            .map(|msgs| msgs.iter().any(|m| {
                let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
                content.contains("generate a title")
                    || content.contains("Generate a concise")
                    || content.contains("### Task:")
                    || content.contains("\"title\"")
                    || content.contains("\"tags\"")
                    || content.contains("\"follow_ups\"")
                    || content.contains("JSON format")
                    || content.contains("json format")
                    || content.contains("broad tags categorizing")
                    || content.contains("emoji as a title")
            }))
            .unwrap_or(false)
    } else {
        false
    };

    // Inject context into /api/chat requests (skip internal/meta requests)
    // Agentic path for /api/chat
    let ollama_agentic = state.agentic_config.agentic().await;
    if path == "/api/chat" && !user_msg.is_empty() && !is_internal_request
        && ollama_agentic.enabled && !crate::agentic::is_disabled_by_header(&headers)
    {
        let is_no_tool_model = state.no_tool_models.lock().await.contains(&model);
        if !is_no_tool_model {
            if let Ok(mut req_json) = serde_json::from_slice::<serde_json::Value>(&body) {
                // Ollama /api/chat needs stream:false for the loop, and uses the Ollama upstream
                return crate::agentic::run_agentic_loop(
                    &state,
                    &headers,
                    &mut req_json,
                    crate::agentic::ApiFormat::Ollama,
                    &format!("http://127.0.0.1:{}", ollama_port),
                    "ollama",
                )
                .await;
            }
        }
    }

    // Fallback: single-shot context injection (original behavior)
    let modified_body = if path == "/api/chat" && !user_msg.is_empty() && !is_internal_request {
        let clean_query = crate::anthropic::sanitize_query_pub(&user_msg);
        let current_session = state.sessions.get_session("ollama").await;
        let injected = match state.engine.search(&clean_query, &state.user_id, 20).await {
            Ok(ref results) => {
                let clean: Vec<_> = results
                    .iter()
                    .filter(|r| !crate::anthropic::is_system_prompt_leak_pub(&r.content))
                    .filter(|r| r.session_id != current_session) // exclude current conversation
                    .cloned()
                    .collect();
                if clean.is_empty() {
                    None
                } else {
                    let ctx = uc_core::assembler::assemble_context(&clean, state.context_budget);
                    if ctx.chunks_included > 0 {
                        eprintln!("[memoryport-proxy] injecting {} chunks into Ollama /api/chat", ctx.chunks_included);
                        Some(crate::anthropic::format_plain_context(&ctx.formatted))
                    } else {
                        None
                    }
                }
            }
            Err(_) => None,
        };

        if let Some(context_text) = injected {
            if let Ok(mut req_json) = serde_json::from_slice::<serde_json::Value>(&body) {
                // Append context to the last user message
                if let Some(messages) = req_json.get_mut("messages").and_then(|m| m.as_array_mut()) {
                    for msg in messages.iter_mut().rev() {
                        if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                            if let Some(content) = msg.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()) {
                                msg["content"] = serde_json::Value::String(format!(
                                    "{}\n\nFor additional context, here is information from my previous conversations:\n\n{}\n\nPlease use the above context to answer my question.",
                                    content, context_text
                                ));
                            }
                            break;
                        }
                    }
                }
                serde_json::to_vec(&req_json).ok()
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let forward_body = modified_body.unwrap_or_else(|| body.to_vec());

    // Forward the request
    let mut req_builder = state.http.request(method, &upstream);
    if let Some(ct) = headers.get("content-type") {
        req_builder = req_builder.header("content-type", ct);
    }
    if !forward_body.is_empty() {
        req_builder = req_builder.body(forward_body);
    }

    let resp = req_builder.send().await.map_err(|e| {
        warn!(error = %e, "failed to forward to Ollama");
        StatusCode::BAD_GATEWAY
    })?;

    let status = StatusCode::from_u16(resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let resp_headers = resp.headers().clone();
    let body_bytes = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    // Capture conversation from /api/chat and /api/generate responses (skip internal requests)
    if status.is_success() && (path == "/api/chat" || path == "/api/generate") && !is_internal_request {
        let user_msg_clean = crate::anthropic::sanitize_for_storage_pub(&user_msg);
        let model_clone = model.clone();

        // Extract assistant response from streaming NDJSON
        let assistant_text = if let Ok(body_str) = std::str::from_utf8(&body_bytes) {
            extract_ollama_response(body_str, &path)
        } else {
            String::new()
        };
        let assistant_clean = crate::anthropic::sanitize_for_storage_pub(&assistant_text);

        // Store user message
        if user_msg_clean.len() >= 10 {
            let engine = state.engine.clone();
            let uid = state.user_id.clone();
            let sid = state.sessions.get_session("ollama").await;
            let m = model_clone.clone();
            let msg = user_msg_clean;
            tokio::spawn(async move {
                let params = uc_core::models::StoreParams {
                    user_id: uid, session_id: sid,
                    chunk_type: uc_core::models::ChunkType::Conversation,
                    role: Some(uc_core::models::Role::User),
                    source_integration: Some("proxy-ollama".into()),
                    source_model: Some(m),
                };
                let _ = engine.store(&msg, params).await;
                let _ = engine.flush().await;
            });
        }

        // Store assistant response
        if assistant_clean.len() >= 10 {
            let engine = state.engine.clone();
            let uid = state.user_id.clone();
            let sid = state.sessions.get_session("ollama").await;
            let m = model_clone;
            tokio::spawn(async move {
                let params = uc_core::models::StoreParams {
                    user_id: uid, session_id: sid,
                    chunk_type: uc_core::models::ChunkType::Conversation,
                    role: Some(uc_core::models::Role::Assistant),
                    source_integration: Some("proxy-ollama".into()),
                    source_model: Some(m),
                };
                let _ = engine.store(&assistant_clean, params).await;
                let _ = engine.flush().await;
            });
        }
    }

    let mut response = (status, body_bytes).into_response();
    if let Some(ct) = resp_headers.get("content-type") {
        response.headers_mut().insert("content-type", ct.clone());
    }
    Ok(response)
}

/// Extract assistant text from Ollama streaming NDJSON response.
fn extract_ollama_response(body: &str, path: &str) -> String {
    let mut text = String::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
            if path == "/api/chat" {
                // /api/chat: {"message":{"content":"token"},"done":false}
                if let Some(content) = obj.get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                {
                    text.push_str(content);
                }
            } else {
                // /api/generate: {"response":"token","done":false}
                if let Some(content) = obj.get("response").and_then(|r| r.as_str()) {
                    text.push_str(content);
                }
            }
        }
    }
    text
}
