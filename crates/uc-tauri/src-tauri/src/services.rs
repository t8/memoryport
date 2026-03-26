use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;
use tauri::AppHandle;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Stopped,
    Starting,
    Running,
    Unhealthy,
    Crashed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    pub name: String,
    pub status: ServiceStatus,
    pub uptime_secs: Option<u64>,
    pub restart_count: u32,
    pub details: Option<String>,
}

struct ManagedService {
    name: String,
    status: ServiceStatus,
    port: u16,
    started_at: Option<Instant>,
    restart_count: u32,
}

impl ManagedService {
    fn new(name: &str, port: u16) -> Self {
        Self {
            name: name.to_string(),
            status: ServiceStatus::Stopped,
            port,
            started_at: None,
            restart_count: 0,
        }
    }

    fn info(&self) -> ServiceInfo {
        ServiceInfo {
            name: self.name.clone(),
            status: self.status.clone(),
            uptime_secs: self.started_at.map(|t| t.elapsed().as_secs()),
            restart_count: self.restart_count,
            details: None,
        }
    }
}

#[derive(Serialize)]
pub struct ServiceHealthResponse {
    pub engine: ServiceInfo,
    pub proxy: ServiceInfo,
    pub mcp: ServiceInfo,
    pub ollama: ServiceInfo,
}

pub struct ServiceManager {
    #[allow(dead_code)]
    app_handle: AppHandle,
    proxy: Arc<Mutex<ManagedService>>,
    server: Arc<Mutex<ManagedService>>,
    http: reqwest::Client,
}

impl ServiceManager {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            proxy: Arc::new(Mutex::new(ManagedService::new("proxy", 9191))),
            server: Arc::new(Mutex::new(ManagedService::new("server", 8090))),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap(),
        }
    }

    pub async fn start_all(&self) {
        // For now, just mark as running — actual sidecar spawning comes in Phase 1b
        // when we wire up the binary bundling
        self.check_health().await;
    }

    pub async fn stop_all(&self) {
        let mut proxy = self.proxy.lock().await;
        proxy.status = ServiceStatus::Stopped;
        proxy.started_at = None;

        let mut server = self.server.lock().await;
        server.status = ServiceStatus::Stopped;
        server.started_at = None;
    }

    pub async fn check_health(&self) {
        // Check proxy
        {
            let mut svc = self.proxy.lock().await;
            match self.http.get("http://127.0.0.1:9191/health").send().await {
                Ok(r) if r.status().is_success() => {
                    if svc.status != ServiceStatus::Running {
                        svc.started_at = Some(Instant::now());
                    }
                    svc.status = ServiceStatus::Running;
                }
                _ => {
                    if svc.status == ServiceStatus::Running {
                        svc.status = ServiceStatus::Crashed;
                    }
                }
            }
        }

        // Check server
        {
            let mut svc = self.server.lock().await;
            match self.http.get("http://127.0.0.1:8090/health").send().await {
                Ok(r) if r.status().is_success() => {
                    if svc.status != ServiceStatus::Running {
                        svc.started_at = Some(Instant::now());
                    }
                    svc.status = ServiceStatus::Running;
                }
                _ => {
                    if svc.status == ServiceStatus::Running {
                        svc.status = ServiceStatus::Crashed;
                    }
                }
            }
        }
    }

    pub async fn health(&self) -> ServiceHealthResponse {
        self.check_health().await;

        let proxy = self.proxy.lock().await.info();
        let server = self.server.lock().await.info();

        // MCP: check config registration
        let mcp_registered = dirs::home_dir()
            .map(|h| h.join(".claude.json"))
            .filter(|p| p.exists())
            .and_then(|p| std::fs::read_to_string(&p).ok())
            .map(|c| c.contains("memoryport") || c.contains("uc-mcp"))
            .unwrap_or(false);

        // Ollama: check if running
        let ollama_running = self
            .http
            .get("http://127.0.0.1:11434")
            .send()
            .await
            .is_ok();

        ServiceHealthResponse {
            engine: server.clone(), // engine runs inside the Tauri process
            proxy,
            mcp: ServiceInfo {
                name: "mcp".to_string(),
                status: if mcp_registered {
                    ServiceStatus::Running
                } else {
                    ServiceStatus::Stopped
                },
                uptime_secs: None,
                restart_count: 0,
                details: if mcp_registered {
                    Some("registered in ~/.claude.json".into())
                } else {
                    Some("not registered".into())
                },
            },
            ollama: ServiceInfo {
                name: "ollama".to_string(),
                status: if ollama_running {
                    ServiceStatus::Running
                } else {
                    ServiceStatus::Stopped
                },
                uptime_secs: None,
                restart_count: 0,
                details: None,
            },
        }
    }
}
