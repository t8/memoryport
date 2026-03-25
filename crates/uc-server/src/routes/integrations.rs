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
