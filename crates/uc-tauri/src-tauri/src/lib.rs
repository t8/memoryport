pub mod commands;

use std::path::PathBuf;
use std::sync::Arc;
use uc_core::config::Config;
use uc_core::Engine;

/// Wrapper to make Engine accessible from Tauri commands.
pub struct AppEngine(pub Arc<Engine>);
/// Tokio runtime for async operations.
pub struct AppRuntime(pub tokio::runtime::Runtime);
/// Config file path for settings read/write.
pub struct AppConfigPath(pub PathBuf);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");

    let config_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".memoryport")
        .join("uc.toml");

    let engine = rt.block_on(async {
        let config = Config::from_file(&config_path)
            .unwrap_or_else(|_| Config::default_config());

        Engine::new(config).await.expect("failed to create engine")
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppEngine(Arc::new(engine)))
        .manage(AppRuntime(rt))
        .manage(AppConfigPath(config_path))
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::list_sessions,
            commands::get_session,
            commands::retrieve,
            commands::store_text,
            commands::get_graph,
            commands::get_analytics,
            commands::get_integrations,
            commands::toggle_integration,
            commands::get_settings,
            commands::update_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Memoryport");
}
