pub mod commands;
pub mod services;

use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;
use uc_core::config::Config;
use uc_core::Engine;

use crate::services::ServiceManager;

/// Engine behind RwLock — None until config exists and init completes.
pub struct AppEngine(pub Arc<RwLock<Option<Arc<Engine>>>>);
/// Tokio runtime for async operations.
pub struct AppRuntime(pub tokio::runtime::Runtime);
/// Config file path for settings read/write.
pub struct AppConfigPath(pub PathBuf);
/// Service manager for proxy/server sidecars.
pub struct AppServices(pub Arc<RwLock<Option<ServiceManager>>>);

/// Helper to get the engine or return an error.
pub async fn get_engine(state: &AppEngine) -> Result<Arc<Engine>, String> {
    state
        .0
        .read()
        .await
        .clone()
        .ok_or_else(|| "Engine not initialized. Complete setup first.".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");

    let config_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".memoryport")
        .join("uc.toml");

    let config_exists = config_path.exists();

    // If config exists, try creating Engine. If it fails, start without it.
    let engine: Option<Arc<Engine>> = if config_exists {
        match rt.block_on(async {
            let config = Config::from_file(&config_path)
                .unwrap_or_else(|_| Config::default_config());
            Engine::new(config).await
        }) {
            Ok(e) => {
                tracing::info!("engine initialized from config");
                Some(Arc::new(e))
            }
            Err(e) => {
                tracing::error!("engine init failed: {e}");
                None
            }
        }
    } else {
        tracing::info!("no config found — starting in setup mode");
        None
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppEngine(Arc::new(RwLock::new(engine))))
        .manage(AppRuntime(rt))
        .manage(AppConfigPath(config_path))
        .manage(AppServices(Arc::new(RwLock::new(None))))
        .setup(move |app| {
            let handle = app.handle().clone();
            let svc = ServiceManager::new(handle);

            let app_services = app.state::<AppServices>();
            let services_lock = app_services.0.clone();

            // Store service manager
            tauri::async_runtime::block_on(async {
                let mut guard = services_lock.write().await;
                *guard = Some(svc);
            });

            // If config exists, start services
            if config_exists {
                let services_lock2: Arc<RwLock<Option<ServiceManager>>> = app_services.0.clone();
                tauri::async_runtime::spawn(async move {
                    let guard = services_lock2.read().await;
                    if let Some(ref svc) = *guard {
                        svc.start_all().await;
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Data commands
            commands::get_status,
            commands::list_sessions,
            commands::get_session,
            commands::retrieve,
            commands::store_text,
            commands::get_graph,
            commands::get_analytics,
            // Integration commands
            commands::get_integrations,
            commands::toggle_integration,
            // Settings commands
            commands::get_settings,
            commands::update_settings,
            // Setup + lifecycle commands
            commands::check_config_exists,
            commands::write_initial_config,
            commands::init_engine,
            commands::get_service_health,
            commands::start_services,
            commands::stop_services,
            commands::restart_service,
            commands::check_ollama_installed,
            commands::install_ollama,
            commands::pull_ollama_model,
            commands::register_mcp,
            commands::register_proxy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Memoryport");
}
