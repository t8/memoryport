use dialoguer::{Confirm, Input, Select};
use std::path::{Path, PathBuf};
use std::process::Command;

const LOGO: &str = "
\x1b[36m ███╗   ███╗███████╗███╗   ███╗ ██████╗ ██████╗ ██╗   ██╗
 ████╗ ████║██╔════╝████╗ ████║██╔═══██╗██╔══██╗╚██╗ ██╔╝
 ██╔████╔██║█████╗  ██╔████╔██║██║   ██║██████╔╝ ╚████╔╝
 ██║╚██╔╝██║██╔══╝  ██║╚██╔╝██║██║   ██║██╔══██╗  ╚██╔╝
 ██║ ╚═╝ ██║███████╗██║ ╚═╝ ██║╚██████╔╝██║  ██║   ██║
 ╚═╝     ╚═╝╚══════╝╚═╝     ╚═╝ ╚═════╝ ╚═╝  ╚═╝   ╚═╝
 ██████╗  ██████╗ ██████╗ ████████╗
 ██╔══██╗██╔═══██╗██╔══██╗╚══██╔══╝
 ██████╔╝██║   ██║██████╔╝   ██║
 ██╔═══╝ ██║   ██║██╔══██╗   ██║
 ██║     ╚██████╔╝██║  ██║   ██║
 ╚═╝      ╚═════╝ ╚═╝  ╚═╝   ╚═╝\x1b[0m

 \x1b[2mDestroyer of the context window\x1b[0m
";

fn dim(s: &str) -> String {
    format!("\x1b[2m{}\x1b[0m", s)
}

fn bold(s: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", s)
}

fn green(s: &str) -> String {
    format!("\x1b[32m{}\x1b[0m", s)
}

fn cyan(s: &str) -> String {
    format!("\x1b[36m{}\x1b[0m", s)
}

fn yellow(s: &str) -> String {
    format!("\x1b[33m{}\x1b[0m", s)
}

fn step(n: u8, msg: &str) {
    println!("  {} {}", cyan(&format!("[{n}/6]")), bold(msg));
}

fn success(msg: &str) {
    println!("     {} {}", green("✓"), msg);
}

fn info(msg: &str) {
    println!("     {} {}", dim("·"), msg);
}

/// Run the interactive setup wizard.
pub fn run_init() -> anyhow::Result<()> {
    println!("{}", LOGO);

    let uc_dir = get_uc_dir();
    let config_path = uc_dir.join("uc.toml");

    // Check existing config
    if config_path.exists() {
        println!("  {} Existing configuration found at {}", yellow("!"), config_path.display());
        println!();
        let reconfigure = Confirm::new()
            .with_prompt("  Reconfigure?")
            .default(false)
            .interact()?;
        if !reconfigure {
            println!("\n  {}", dim("Keeping existing configuration. Done."));
            return Ok(());
        }
        println!();
    }

    // ── Step 1: Choose embedding provider ──
    step(1, "Choose your embedding provider");
    println!();
    println!("     Memoryport needs an embedding model to understand your text.");
    println!("     Embeddings convert words into numbers so we can find");
    println!("     semantically similar content — this is how your AI recalls");
    println!("     relevant context from past conversations.");
    println!();
    let provider_options = &[
        "OpenAI  — cloud embeddings, requires API key",
        "Ollama  — local embeddings, free, private",
    ];
    let provider_idx = Select::new()
        .with_prompt("  Provider")
        .items(provider_options)
        .default(0)
        .interact()?;

    println!();
    let (provider, model, dimensions, api_key) = match provider_idx {
        0 => setup_openai()?,
        1 => setup_ollama()?,
        _ => unreachable!(),
    };

    // ── Step 2: Cloud storage (optional API key) ──
    println!();
    step(2, "Cloud storage");
    println!();
    println!("     Memoryport can back up your memory permanently to Arweave.");
    println!("     This requires a Pro subscription at {}", bold("memoryport.ai"));
    println!();
    let uc_api_key_input: String = dialoguer::Input::new()
        .with_prompt("  API key (press Enter to skip)")
        .allow_empty(true)
        .interact_text()?;
    let uc_api_key: Option<String> = if uc_api_key_input.starts_with("uc_") {
        success("API key configured");
        Some(uc_api_key_input)
    } else if uc_api_key_input.is_empty() {
        info("Skipped — local-only mode (you can add an API key later)");
        None
    } else {
        println!("     {} API key should start with uc_", yellow("!"));
        info("Skipped — local-only mode");
        None
    };

    // ── Step 3: Create directories + config ──
    println!();
    step(3, "Writing configuration");
    std::fs::create_dir_all(uc_dir.join("index"))?;
    let config_content = generate_config(
        &provider,
        &model,
        dimensions,
        api_key.as_deref(),
        uc_api_key.as_deref(),
        &uc_dir,
    );
    std::fs::write(&config_path, &config_content)?;
    success(&format!("Config written to {}", config_path.display()));
    success(&format!("Index directory at {}/index", uc_dir.display()));

    // ── Step 4: Register MCP servers ──
    println!();
    step(4, "Registering MCP server in editors");
    register_mcp_servers(&uc_dir)?;

    // ── Step 5: Auto-capture proxy ──
    println!();
    step(5, "Auto-capture proxy");
    println!();
    println!("     The proxy captures every conversation automatically —");
    println!("     both your messages and the AI's responses. It sits");
    println!("     between your editor and the AI provider, transparent");
    println!("     and invisible.");
    println!();
    let enable_proxy = Confirm::new()
        .with_prompt("  Enable auto-capture proxy?")
        .default(true)
        .interact()?;

    if enable_proxy {
        setup_proxy(&config_path, &uc_dir)?;
    } else {
        info("Proxy not enabled. You can enable it later by re-running uc init.");
    }

    // ── Step 6: Summary ──
    println!();
    step(6, "Done!");
    println!();
    println!();
    println!("  {} Memoryport is ready.", green("✓"));
    println!();
    success(&format!("Config:   {}", config_path.display()));
    success(&format!("Provider: {provider} / {model}"));
    if uc_api_key.is_some() {
        success("Arweave:  Pro storage enabled (wallet will be generated on first run)");
    } else {
        success("Storage:  local-only");
    }
    if enable_proxy {
        success("Proxy:    auto-capture enabled");
    }
    println!();
    println!("  Next steps:");
    if enable_proxy {
        println!("    1. Start the proxy:");
        println!("       {}", bold(&format!("uc-proxy --config {}", config_path.display())));
        println!("    2. Restart your editor (Claude Code / Cursor)");
        println!("    3. Start chatting — memory is automatic");
    } else {
        println!("    1. Restart your editor (Claude Code / Cursor)");
        println!("    2. Start chatting — memory is automatic");
    }
    println!();
    println!("  Test: {}", dim(&format!("uc --config {} status", config_path.display())));
    println!();

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
        success("Found OPENAI_API_KEY in environment");
        let use_existing = Confirm::new()
            .with_prompt("  Use existing key?")
            .default(true)
            .interact()?;
        if use_existing {
            None
        } else {
            let key: String = Input::new()
                .with_prompt("  OpenAI API key")
                .interact_text()?;
            Some(key)
        }
    } else {
        info("No OPENAI_API_KEY found in environment");
        let key: String = Input::new()
            .with_prompt("  Enter OpenAI API key (or leave empty to set OPENAI_API_KEY later)")
            .allow_empty(true)
            .interact_text()?;
        if key.is_empty() {
            println!("     {} Set OPENAI_API_KEY before using Memoryport", yellow("!"));
            None
        } else {
            success("API key saved to config");
            Some(key)
        }
    };

    success(&format!("Using {} with {}", bold("OpenAI"), bold("text-embedding-3-small")));

    Ok(("openai".into(), "text-embedding-3-small".into(), 1536, api_key))
}

fn setup_ollama() -> anyhow::Result<(String, String, usize, Option<String>)> {
    let ollama_installed = Command::new("ollama").arg("--version").output().is_ok();

    if !ollama_installed {
        println!("     {} Ollama is not installed", yellow("!"));
        let install = Confirm::new()
            .with_prompt("  Install Ollama now?")
            .default(true)
            .interact()?;

        if install {
            info("Installing Ollama...");
            let status = Command::new("sh")
                .arg("-c")
                .arg("curl -fsSL https://ollama.com/install.sh | sh")
                .status();

            match status {
                Ok(s) if s.success() => success("Ollama installed"),
                _ => {
                    println!("     {} Install failed. Visit https://ollama.com", yellow("!"));
                    info("Continuing with Ollama configuration...");
                }
            }
        } else {
            info("Install Ollama later: https://ollama.com");
        }
    } else {
        success("Ollama detected");
    }

    info("Pulling nomic-embed-text model...");
    let pull_result = Command::new("ollama")
        .args(["pull", "nomic-embed-text"])
        .status();

    match pull_result {
        Ok(s) if s.success() => success("Model nomic-embed-text ready"),
        _ => println!("     {} Run `ollama pull nomic-embed-text` later", yellow("!")),
    }

    success(&format!("Using {} with {}", bold("Ollama"), bold("nomic-embed-text")));

    Ok(("ollama".into(), "nomic-embed-text".into(), 768, None))
}

fn generate_config(
    provider: &str,
    model: &str,
    dimensions: usize,
    api_key: Option<&str>,
    uc_api_key: Option<&str>,
    uc_dir: &Path,
) -> String {
    let mut config = "[arweave]\ngateway = \"https://arweave.net\"\nturbo_endpoint = \"https://upload.ardrive.io\"\n".to_string();

    if let Some(key) = uc_api_key {
        config.push_str(&format!("api_key = \"{key}\"\n"));
        config.push_str(&format!(
            "wallet_path = \"{}/wallet.json\"\n",
            uc_dir.display()
        ));
    }

    {
        let dir = uc_dir.display();
        config.push_str(&format!(
            "\n[index]\npath = \"{dir}/index\"\nembedding_dimensions = {dimensions}\n\n[embeddings]\nprovider = \"{provider}\"\nmodel = \"{model}\"\ndimensions = {dimensions}\n"
        ));
    }

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

fn setup_proxy(config_path: &Path, _uc_dir: &Path) -> anyhow::Result<()> {
    let proxy_listen = "127.0.0.1:9191";

    // Append proxy config to uc.toml
    let mut config = std::fs::read_to_string(config_path)?;
    if !config.contains("[proxy]") {
        config.push_str(&format!(
            r#"
[proxy]
listen = "{proxy_listen}"
"#
        ));
        std::fs::write(config_path, &config)?;
    }
    success(&format!("Proxy configured on {proxy_listen}"));

    // Set ANTHROPIC_BASE_URL in ~/.claude.json
    let claude_json = dirs_home().join(".claude.json");
    if claude_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&claude_json) {
            if let Ok(mut data) = serde_json::from_str::<serde_json::Value>(&content) {
                let env = data
                    .as_object_mut()
                    .unwrap()
                    .entry("env")
                    .or_insert(serde_json::json!({}));
                env.as_object_mut()
                    .unwrap()
                    .insert(
                        "ANTHROPIC_BASE_URL".into(),
                        serde_json::json!(format!("http://{proxy_listen}")),
                    );
                let updated = serde_json::to_string_pretty(&data)?;
                std::fs::write(&claude_json, updated)?;
                success("Set ANTHROPIC_BASE_URL in Claude Code config");
            }
        }
    } else {
        info(&format!(
            "Set this env var for Claude Code: ANTHROPIC_BASE_URL=http://{proxy_listen}"
        ));
    }

    let proxy_bin = find_proxy_binary();
    info(&format!("Proxy binary: {proxy_bin}"));

    Ok(())
}

fn find_proxy_binary() -> String {
    let in_path = which("uc-proxy");
    if !in_path.is_empty() && Path::new(&in_path).exists() {
        return in_path;
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let sibling = dir.join("uc-proxy");
            if sibling.exists() {
                return sibling.to_string_lossy().to_string();
            }
        }
    }
    "uc-proxy".into()
}

fn register_mcp_servers(uc_dir: &Path) -> anyhow::Result<()> {
    let uc_mcp_path = find_uc_mcp_binary();

    // Claude Code: MCP servers go in ~/.claude.json (not settings.json)
    let claude_json = dirs_home().join(".claude.json");
    register_editor_mcp(&claude_json, &uc_mcp_path, uc_dir, "Claude Code")?;

    // Cursor MCP config
    let cursor_path = dirs_home().join(".cursor").join("mcp.json");
    if cursor_path.parent().map_or(false, |p| p.exists()) {
        register_editor_mcp(&cursor_path, &uc_mcp_path, uc_dir, "Cursor")?;
    }

    Ok(())
}

fn register_editor_mcp(
    settings_path: &Path,
    uc_mcp_path: &str,
    uc_dir: &Path,
    editor_name: &str,
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
    success(&format!("Registered in {} ({})", editor_name, settings_path.display()));

    Ok(())
}

fn find_uc_mcp_binary() -> String {
    // Check if uc-mcp is in PATH (returns absolute path)
    let in_path = which("uc-mcp");
    if !in_path.is_empty() && Path::new(&in_path).exists() {
        return in_path;
    }

    // Check alongside the current binary
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let sibling = dir.join("uc-mcp");
            if sibling.exists() {
                return sibling.to_string_lossy().to_string();
            }
        }
    }

    // Check in the memoryport bin dir
    let mp_bin = get_uc_dir().join("bin").join("uc-mcp");
    if mp_bin.exists() {
        return mp_bin.to_string_lossy().to_string();
    }

    // Fallback
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
