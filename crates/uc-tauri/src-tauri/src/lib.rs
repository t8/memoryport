pub mod commands;

use std::sync::Arc;
use uc_core::config::Config;
use uc_core::Engine;

/// Wrapper to make Engine accessible from Tauri commands.
pub struct AppEngine(pub Arc<Engine>);
/// Tokio runtime for async operations.
pub struct AppRuntime(pub tokio::runtime::Runtime);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");

    let engine = rt.block_on(async {
        let config_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".memoryport")
            .join("uc.toml");

        let config = Config::from_file(&config_path)
            .unwrap_or_else(|_| Config::default_config());

        Engine::new(config).await.expect("failed to create engine")
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppEngine(Arc::new(engine)))
        .manage(AppRuntime(rt))
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::list_sessions,
            commands::get_session,
            commands::retrieve,
            commands::store_text,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Memoryport");
}
