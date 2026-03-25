use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct IntegrationsStatus {
    pub mcp: IntegrationState,
    pub proxy: IntegrationState,
    pub ollama: IntegrationState,
    pub arweave: IntegrationState,
}

#[derive(Debug, Serialize)]
pub struct IntegrationState {
    pub enabled: bool,
    pub status: String, // "operational", "down", "unconfigured"
}

#[derive(Debug, Deserialize)]
pub struct ToggleRequest {
    pub integration: String, // "mcp", "proxy", "arweave"
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct ToggleResponse {
    pub success: bool,
    pub message: String,
}

/// GET /v1/integrations — current state of all integrations
pub async fn get_integrations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<IntegrationsStatus>, ApiError> {
    let config = state.pool.base_config();

    // Check MCP: is it registered in ~/.claude.json?
    let mcp_enabled = check_mcp_registered();

    // Check proxy: is ANTHROPIC_BASE_URL set in claude config?
    // (more reliable than health check which races startup)
    let proxy_enabled = check_proxy_configured();

    // Check Ollama: is the proxy intercepting on port 11434?
    let ollama_enabled = check_ollama_intercept_active().await;

    // Check Arweave: is wallet configured?
    let arweave_enabled = config.arweave.wallet_path.is_some();

    Ok(Json(IntegrationsStatus {
        mcp: IntegrationState {
            enabled: mcp_enabled,
            status: if mcp_enabled { "operational".into() } else { "unconfigured".into() },
        },
        proxy: IntegrationState {
            enabled: proxy_enabled,
            status: if proxy_enabled { "operational".into() } else { "unconfigured".into() },
        },
        ollama: IntegrationState {
            enabled: ollama_enabled,
            status: if ollama_enabled { "operational".into() } else { "unconfigured".into() },
        },
        arweave: IntegrationState {
            enabled: arweave_enabled,
            status: if arweave_enabled { "operational".into() } else { "unconfigured".into() },
        },
    }))
}

/// POST /v1/integrations/toggle — enable or disable an integration
pub async fn toggle_integration(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ToggleRequest>,
) -> Result<Json<ToggleResponse>, ApiError> {
    match req.integration.as_str() {
        "mcp" => toggle_mcp(req.enabled),
        "proxy" => toggle_proxy(req.enabled, state.pool.base_config()).await,
        "ollama" => toggle_ollama(req.enabled).await,
        "arweave" => Ok(ToggleResponse {
            success: false,
            message: "Arweave requires a wallet. Configure wallet_path in settings.".into(),
        }),
        _ => Ok(ToggleResponse {
            success: false,
            message: format!("Unknown integration: {}", req.integration),
        }),
    }
    .map(Json)
}

fn toggle_mcp(enabled: bool) -> Result<ToggleResponse, ApiError> {
    let claude_json_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude.json");

    if !claude_json_path.exists() {
        return Ok(ToggleResponse {
            success: false,
            message: "~/.claude.json not found. Run `uc init` first.".into(),
        });
    }

    let content = std::fs::read_to_string(&claude_json_path)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if enabled {
        // Find uc-mcp binary
        let mcp_bin = find_uc_mcp_binary();
        let config_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".memoryport")
            .join("uc.toml")
            .to_string_lossy()
            .to_string();

        data.as_object_mut()
            .unwrap()
            .entry("mcpServers")
            .or_insert(serde_json::json!({}))
            .as_object_mut()
            .unwrap()
            .insert(
                "memoryport".into(),
                serde_json::json!({
                    "command": mcp_bin,
                    "args": ["--config", config_path]
                }),
            );
    } else {
        if let Some(servers) = data.get_mut("mcpServers").and_then(|s| s.as_object_mut()) {
            servers.remove("memoryport");
        }
    }

    let updated = serde_json::to_string_pretty(&data)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    std::fs::write(&claude_json_path, updated)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(ToggleResponse {
        success: true,
        message: if enabled {
            "MCP server registered. Restart Claude Code to connect.".into()
        } else {
            "MCP server removed. Restart Claude Code to disconnect.".into()
        },
    })
}

async fn toggle_proxy(
    enabled: bool,
    config: &uc_core::config::Config,
) -> Result<ToggleResponse, ApiError> {
    let claude_json_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude.json");

    if !claude_json_path.exists() {
        return Ok(ToggleResponse {
            success: false,
            message: "~/.claude.json not found. Run `uc init` first.".into(),
        });
    }

    let content = std::fs::read_to_string(&claude_json_path)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Proxy runs on its own port, not the server's listen port
    let proxy_url = "http://127.0.0.1:9191".to_string();

    if enabled {
        // Set ANTHROPIC_BASE_URL in env
        data.as_object_mut()
            .unwrap()
            .entry("env")
            .or_insert(serde_json::json!({}))
            .as_object_mut()
            .unwrap()
            .insert("ANTHROPIC_BASE_URL".into(), serde_json::json!(proxy_url));

        // Start proxy process in background
        let proxy_bin = find_uc_proxy_binary();
        let config_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".memoryport")
            .join("uc.toml")
            .to_string_lossy()
            .to_string();

        std::process::Command::new(&proxy_bin)
            .arg("--config")
            .arg(&config_path)
            .arg("--listen")
            .arg("127.0.0.1:9191")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| ApiError::Internal(format!("Failed to start proxy: {e}")))?;
    } else {
        // Remove ANTHROPIC_BASE_URL
        if let Some(env) = data.get_mut("env").and_then(|e| e.as_object_mut()) {
            env.remove("ANTHROPIC_BASE_URL");
        }

        // Kill proxy process
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("pkill")
                .arg("-f")
                .arg("uc-proxy")
                .status();
        }
    }

    let updated = serde_json::to_string_pretty(&data)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    std::fs::write(&claude_json_path, updated)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(ToggleResponse {
        success: true,
        message: if enabled {
            "Proxy started and ANTHROPIC_BASE_URL configured. Restart Claude Code to activate.".into()
        } else {
            "Proxy stopped and ANTHROPIC_BASE_URL removed. Restart Claude Code to deactivate.".into()
        },
    })
}

/// Check if the proxy is intercepting on Ollama's default port (11434).
/// We detect this by hitting :11434/health — if it responds with "ok" it's our proxy.
/// Real Ollama responds with a different format.
async fn check_ollama_intercept_active() -> bool {
    // Check if something is on 11434 that identifies as our proxy
    let memoryport_marker = dirs::home_dir()
        .unwrap_or_default()
        .join(".memoryport")
        .join("ollama-intercept.active");
    memoryport_marker.exists()
}

const OLLAMA_DEFAULT_PORT: u16 = 11434;
const OLLAMA_MOVED_PORT: u16 = 11435;

async fn toggle_ollama(enabled: bool) -> Result<ToggleResponse, ApiError> {
    let marker = dirs::home_dir()
        .unwrap_or_default()
        .join(".memoryport")
        .join("ollama-intercept.active");

    if enabled {
        // Step 1: Check if Ollama is running
        let ollama_running = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{OLLAMA_DEFAULT_PORT}"))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .is_ok();

        if !ollama_running {
            return Ok(ToggleResponse {
                success: false,
                message: "Ollama is not running on port 11434. Start Ollama first.".into(),
            });
        }

        // Step 2: Stop Ollama completely (including macOS app auto-restart)
        #[cfg(target_os = "macos")]
        {
            // Quit the Ollama.app gracefully first (prevents launchd respawn)
            let _ = std::process::Command::new("osascript")
                .arg("-e")
                .arg("tell application \"Ollama\" to quit")
                .status();
            std::thread::sleep(std::time::Duration::from_secs(2));
            // Also kill any remaining ollama serve processes
            let _ = std::process::Command::new("pkill").arg("-9").arg("-f").arg("ollama serve").status();
            std::thread::sleep(std::time::Duration::from_secs(1));
            // Kill anything still on 11434
            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg("lsof -ti:11434 | xargs kill -9 2>/dev/null")
                .status();
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = std::process::Command::new("pkill").arg("-9").arg("-f").arg("ollama").status();
            std::thread::sleep(std::time::Duration::from_secs(2));
        }

        // Step 3: Start ollama serve on the moved port
        let ollama_bin = find_ollama_binary();
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "OLLAMA_HOST=127.0.0.1:{OLLAMA_MOVED_PORT} nohup {ollama_bin} serve > /dev/null 2>&1 &"
            ))
            .status();
        std::thread::sleep(std::time::Duration::from_secs(3));

        // Verify Ollama is on the moved port
        let moved_ok = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{OLLAMA_MOVED_PORT}"))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .is_ok();

        if !moved_ok {
            return Ok(ToggleResponse {
                success: false,
                message: format!("Failed to restart Ollama on port {OLLAMA_MOVED_PORT}. Try again."),
            });
        }

        // Step 4: Start the proxy on port 11434
        let proxy_bin = find_uc_proxy_binary();
        let config_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".memoryport")
            .join("uc.toml")
            .to_string_lossy()
            .to_string();

        std::process::Command::new(&proxy_bin)
            .arg("--config")
            .arg(&config_path)
            .arg("--listen")
            .arg(format!("127.0.0.1:{OLLAMA_DEFAULT_PORT}"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| ApiError::Internal(format!("Failed to start proxy on 11434: {e}")))?;

        // Write marker file
        let _ = std::fs::write(&marker, "active");

        Ok(ToggleResponse {
            success: true,
            message: format!(
                "Ollama moved to port {OLLAMA_MOVED_PORT}. Memoryport proxy intercepts on port {OLLAMA_DEFAULT_PORT}. All Ollama clients work automatically. Note: the Ollama menu bar app will reappear when you toggle this off."
            ),
        })
    } else {
        // Step 1: Kill the proxy on 11434
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg("lsof -ti:11434 | xargs kill -9 2>/dev/null")
            .status();
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Step 2: Kill ollama on moved port
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("lsof -ti:{OLLAMA_MOVED_PORT} | xargs kill -9 2>/dev/null"))
            .status();
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Step 3: Restart Ollama normally
        #[cfg(target_os = "macos")]
        {
            // Reopen Ollama.app (launches with default port)
            let _ = std::process::Command::new("open")
                .arg("-a")
                .arg("Ollama")
                .status();
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg("nohup ollama serve > /dev/null 2>&1 &")
                .status();
        }

        // Remove marker
        let _ = std::fs::remove_file(&marker);

        Ok(ToggleResponse {
            success: true,
            message: "Ollama restored to default port 11434. Proxy intercept disabled.".into(),
        })
    }
}

fn check_mcp_registered() -> bool {
    let claude_json = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude.json");
    if let Ok(content) = std::fs::read_to_string(&claude_json) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            return data
                .get("mcpServers")
                .and_then(|s| s.get("memoryport"))
                .is_some();
        }
    }
    false
}

fn check_proxy_configured() -> bool {
    let claude_json = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude.json");
    if let Ok(content) = std::fs::read_to_string(&claude_json) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            return data
                .get("env")
                .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
                .is_some();
        }
    }
    false
}

fn find_ollama_binary() -> String {
    for path in &["/usr/local/bin/ollama", "/opt/homebrew/bin/ollama"] {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }
    which_binary("ollama")
}

fn find_uc_mcp_binary() -> String {
    which_binary("uc-mcp")
}

fn find_uc_proxy_binary() -> String {
    which_binary("uc-proxy")
}

fn which_binary(name: &str) -> String {
    // Check alongside the current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(name);
            if sibling.exists() {
                return sibling.to_string_lossy().to_string();
            }
        }
    }
    // Check PATH
    if let Ok(output) = std::process::Command::new("which").arg(name).output() {
        if output.status.success() {
            if let Ok(path) = String::from_utf8(output.stdout) {
                let path = path.trim();
                if !path.is_empty() {
                    return path.to_string();
                }
            }
        }
    }
    name.into()
}
