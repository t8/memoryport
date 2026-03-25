use dialoguer::{Confirm, Input, Select};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Run the interactive setup wizard.
pub fn run_init() -> anyhow::Result<()> {
    println!("\n  Memoryport — Setup Wizard\n");

    let uc_dir = get_uc_dir();
    let config_path = uc_dir.join("uc.toml");

    // Check existing config
    if config_path.exists() {
        let reconfigure = Confirm::new()
            .with_prompt("Existing configuration found. Reconfigure?")
            .default(false)
            .interact()?;
        if !reconfigure {
            println!("Keeping existing configuration.");
            return Ok(());
        }
    }

    // Choose embedding provider
    let provider_options = &["OpenAI (requires API key)", "Ollama (local, free)"];
    let provider_idx = Select::new()
        .with_prompt("Choose embedding provider")
        .items(provider_options)
        .default(0)
        .interact()?;

    let (provider, model, dimensions, api_key) = match provider_idx {
        0 => setup_openai()?,
        1 => setup_ollama()?,
        _ => unreachable!(),
    };

    // Create directories
    std::fs::create_dir_all(uc_dir.join("index"))?;
    println!("  Created {}", uc_dir.display());

    // Write config
    let config_content = generate_config(&provider, &model, dimensions, api_key.as_deref(), &uc_dir);
    std::fs::write(&config_path, &config_content)?;
    println!("  Wrote {}", config_path.display());

    // Auto-register MCP server
    register_mcp_servers(&uc_dir)?;

    println!("\n  Setup complete! Memoryport is ready.\n");
    println!("  Config:  {}", config_path.display());
    println!("  Index:   {}", uc_dir.join("index").display());
    println!("\n  To use with Claude Code, restart your editor.");
    println!("  To test: uc --config {} status\n", config_path.display());

    Ok(())
}

fn get_uc_dir() -> PathBuf {
    if let Some(base) = directories::BaseDirs::new() {
        base.home_dir().join(".memoryport")
    } else {
        PathBuf::from(".memoryport")
    }
}

fn setup_openai() -> anyhow::Result<(String, String, usize, Option<String>)> {
    let existing_key = std::env::var("OPENAI_API_KEY").ok();

    let api_key = if existing_key.is_some() {
        println!("  Found OPENAI_API_KEY in environment.");
        let use_existing = Confirm::new()
            .with_prompt("Use existing OPENAI_API_KEY?")
            .default(true)
            .interact()?;
        if use_existing {
            None // will use env var at runtime
        } else {
            let key: String = Input::new()
                .with_prompt("Enter OpenAI API key")
                .interact_text()?;
            Some(key)
        }
    } else {
        let key: String = Input::new()
            .with_prompt("Enter OpenAI API key (or set OPENAI_API_KEY env var)")
            .allow_empty(true)
            .interact_text()?;
        if key.is_empty() {
            println!("  No API key provided. Set OPENAI_API_KEY before using.");
            None
        } else {
            Some(key)
        }
    };

    Ok(("openai".into(), "text-embedding-3-small".into(), 1536, api_key))
}

fn setup_ollama() -> anyhow::Result<(String, String, usize, Option<String>)> {
    // Check if ollama is installed
    let ollama_installed = Command::new("ollama").arg("--version").output().is_ok();

    if !ollama_installed {
        println!("  Ollama is not installed.");
        let install = Confirm::new()
            .with_prompt("Install Ollama now?")
            .default(true)
            .interact()?;

        if install {
            println!("  Installing Ollama...");
            let status = if cfg!(target_os = "macos") {
                // On macOS, use the official installer
                Command::new("sh")
                    .arg("-c")
                    .arg("curl -fsSL https://ollama.com/install.sh | sh")
                    .status()
            } else {
                Command::new("sh")
                    .arg("-c")
                    .arg("curl -fsSL https://ollama.com/install.sh | sh")
                    .status()
            };

            match status {
                Ok(s) if s.success() => println!("  Ollama installed successfully."),
                _ => {
                    println!("  Failed to install Ollama. Please install manually: https://ollama.com");
                    println!("  Continuing with Ollama configuration anyway...");
                }
            }
        } else {
            println!("  Please install Ollama manually: https://ollama.com");
            println!("  Continuing with Ollama configuration...");
        }
    } else {
        println!("  Ollama detected.");
    }

    // Pull the embedding model
    println!("  Pulling nomic-embed-text model...");
    let pull_result = Command::new("ollama")
        .args(["pull", "nomic-embed-text"])
        .status();

    match pull_result {
        Ok(s) if s.success() => println!("  Model ready."),
        _ => println!("  Could not pull model now. Run `ollama pull nomic-embed-text` later."),
    }

    Ok(("ollama".into(), "nomic-embed-text".into(), 768, None))
}

fn generate_config(
    provider: &str,
    model: &str,
    dimensions: usize,
    api_key: Option<&str>,
    uc_dir: &Path,
) -> String {
    let mut config = format!(
        r#"[arweave]
gateway = "https://arweave.net"
turbo_endpoint = "https://upload.ardrive.io"

[index]
path = "{}/index"
embedding_dimensions = {dimensions}

[embeddings]
provider = "{provider}"
model = "{model}"
dimensions = {dimensions}
"#,
        uc_dir.display()
    );

    if let Some(key) = api_key {
        config.push_str(&format!("api_key = \"{key}\"\n"));
    }

    config.push_str(
        r#"
[retrieval]
max_context_tokens = 50000
recency_window = 20
similarity_top_k = 50
"#,
    );

    config
}

fn register_mcp_servers(uc_dir: &Path) -> anyhow::Result<()> {
    let uc_mcp_path = find_uc_mcp_binary();

    // Claude Code settings
    let claude_paths = [
        dirs_home().join(".claude").join("settings.json"),
    ];

    for settings_path in &claude_paths {
        if let Some(parent) = settings_path.parent() {
            if parent.exists() {
                register_claude_code(settings_path, &uc_mcp_path, uc_dir)?;
            }
        }
    }

    // Cursor MCP config
    let cursor_path = dirs_home().join(".cursor").join("mcp.json");
    if cursor_path.parent().map_or(false, |p| p.exists()) {
        register_cursor(&cursor_path, &uc_mcp_path, uc_dir)?;
    }

    Ok(())
}

fn register_claude_code(
    settings_path: &Path,
    uc_mcp_path: &str,
    uc_dir: &Path,
) -> anyhow::Result<()> {
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(settings_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let config_path = uc_dir.join("uc.toml").to_string_lossy().to_string();

    let mcp_entry = serde_json::json!({
        "command": uc_mcp_path,
        "args": ["--config", config_path]
    });

    settings
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert(serde_json::json!({}))
        .as_object_mut()
        .unwrap()
        .insert("memoryport".into(), mcp_entry);

    let content = serde_json::to_string_pretty(&settings)?;
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(settings_path, content)?;
    println!("  Registered MCP server in {}", settings_path.display());

    Ok(())
}

fn register_cursor(
    mcp_path: &Path,
    uc_mcp_path: &str,
    uc_dir: &Path,
) -> anyhow::Result<()> {
    let mut config: serde_json::Value = if mcp_path.exists() {
        let content = std::fs::read_to_string(mcp_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let config_path = uc_dir.join("uc.toml").to_string_lossy().to_string();

    let mcp_entry = serde_json::json!({
        "command": uc_mcp_path,
        "args": ["--config", config_path]
    });

    config
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert(serde_json::json!({}))
        .as_object_mut()
        .unwrap()
        .insert("memoryport".into(), mcp_entry);

    let content = serde_json::to_string_pretty(&config)?;
    std::fs::write(mcp_path, content)?;
    println!("  Registered MCP server in {}", mcp_path.display());

    Ok(())
}

fn find_uc_mcp_binary() -> String {
    // Check common locations
    let candidates = [
        which("uc-mcp"),
        get_uc_dir().join("bin").join("uc-mcp").to_string_lossy().to_string(),
    ];

    for candidate in &candidates {
        if !candidate.is_empty() && Path::new(candidate).exists() {
            return candidate.clone();
        }
    }

    // Fallback: assume it's in PATH
    "uc-mcp".into()
}

fn which(name: &str) -> String {
    Command::new("which")
        .arg(name)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn dirs_home() -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}
