use serde::Serialize;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::{Child, Command};
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
    started_at: Option<Instant>,
    restart_count: u32,
    user_stopped: bool,
    process: Option<Child>,
}

impl ManagedService {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: ServiceStatus::Stopped,
            started_at: None,
            restart_count: 0,
            user_stopped: false,
            process: None,
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

    async fn kill(&mut self) {
        if let Some(ref mut child) = self.process {
            let _ = child.kill().await;
            self.process = None;
        }
        self.status = ServiceStatus::Stopped;
        self.started_at = None;
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
    config_path: PathBuf,
    proxy: Arc<Mutex<ManagedService>>,
    server: Arc<Mutex<ManagedService>>,
    http: reqwest::Client,
}

impl ServiceManager {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            proxy: Arc::new(Mutex::new(ManagedService::new("proxy"))),
            server: Arc::new(Mutex::new(ManagedService::new("server"))),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap(),
        }
    }

    /// Find a binary by name — checks cargo target/debug, then PATH, then ~/.memoryport/bin
    fn find_binary(&self, name: &str) -> Option<PathBuf> {
        // Dev mode: check target/debug relative to the workspace
        let dev_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join(name)))
            .filter(|p| p.exists());
        if dev_path.is_some() {
            return dev_path;
        }

        // PATH
        if let Ok(p) = which::which(name) {
            return Some(p);
        }

        // ~/.memoryport/bin
        let home_bin = dirs::home_dir().map(|h| h.join(".memoryport/bin").join(name)).filter(|p| p.exists());
        if home_bin.is_some() {
            return home_bin;
        }

        None
    }

    pub async fn start_all(&self) {
        let config = self.config_path.to_string_lossy().to_string();

        // Start server
        {
            let mut svc = self.server.lock().await;
            svc.user_stopped = false;
            if svc.process.is_none() {
                if let Some(bin) = self.find_binary("uc-server") {
                    tracing::info!("starting uc-server from {}", bin.display());
                    match Command::new(&bin)
                        .arg("--config")
                        .arg(&config)
                        .env("UC_SERVER_LISTEN", "127.0.0.1:8090")
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .kill_on_drop(true)
                        .spawn()
                    {
                        Ok(child) => {
                            svc.process = Some(child);
                            svc.status = ServiceStatus::Starting;
                            svc.started_at = Some(Instant::now());
                        }
                        Err(e) => tracing::error!("failed to start uc-server: {e}"),
                    }
                } else {
                    tracing::warn!("uc-server binary not found");
                }
            }
        }

        // Start proxy
        {
            let mut svc = self.proxy.lock().await;
            svc.user_stopped = false;
            if svc.process.is_none() {
                if let Some(bin) = self.find_binary("uc-proxy") {
                    tracing::info!("starting uc-proxy from {}", bin.display());
                    match Command::new(&bin)
                        .arg("--config")
                        .arg(&config)
                        .arg("--listen")
                        .arg("127.0.0.1:9191")
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .kill_on_drop(true)
                        .spawn()
                    {
                        Ok(child) => {
                            svc.process = Some(child);
                            svc.status = ServiceStatus::Starting;
                            svc.started_at = Some(Instant::now());
                        }
                        Err(e) => tracing::error!("failed to start uc-proxy: {e}"),
                    }
                } else {
                    tracing::warn!("uc-proxy binary not found");
                }
            }
        }

        // Give processes a moment to start, then check health
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        self.check_health().await;
    }

    pub async fn start_proxy(&self) {
        let config = self.config_path.to_string_lossy().to_string();
        let mut svc = self.proxy.lock().await;
        svc.user_stopped = false;
        if svc.process.is_none() {
            if let Some(bin) = self.find_binary("uc-proxy") {
                tracing::info!("starting uc-proxy from {}", bin.display());
                match Command::new(&bin)
                    .arg("--config")
                    .arg(&config)
                    .arg("--listen")
                    .arg("127.0.0.1:9191")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                {
                    Ok(child) => {
                        svc.process = Some(child);
                        svc.status = ServiceStatus::Starting;
                        svc.started_at = Some(Instant::now());
                    }
                    Err(e) => tracing::error!("failed to start uc-proxy: {e}"),
                }
            }
        }
    }

    pub async fn stop_proxy(&self) {
        let mut svc = self.proxy.lock().await;
        svc.user_stopped = true;
        svc.kill().await;
    }

    pub async fn stop_all(&self) {
        {
            let mut svc = self.proxy.lock().await;
            svc.user_stopped = true;
            svc.kill().await;
        }
        {
            let mut svc = self.server.lock().await;
            svc.user_stopped = true;
            svc.kill().await;
        }
    }

    pub async fn check_health(&self) {
        // Check proxy
        {
            let mut svc = self.proxy.lock().await;
            if !svc.user_stopped {
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
        }

        // Check server
        {
            let mut svc = self.server.lock().await;
            if !svc.user_stopped {
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
    }

    pub async fn health(&self) -> ServiceHealthResponse {
        self.check_health().await;

        let proxy = self.proxy.lock().await.info();
        let server = self.server.lock().await.info();

        // MCP: check config registration (structural check, not string search)
        let mcp_registered = dirs::home_dir()
            .map(|h| h.join(".claude.json"))
            .filter(|p| p.exists())
            .and_then(|p| std::fs::read_to_string(&p).ok())
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|d| d.get("mcpServers")?.as_object()?.contains_key("memoryport").then_some(true))
            .unwrap_or(false);

        // Ollama: check if reachable
        let ollama_running = self
            .http
            .get("http://127.0.0.1:11434")
            .send()
            .await
            .is_ok();

        ServiceHealthResponse {
            engine: ServiceInfo {
                name: "engine".to_string(),
                ..server
            },
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
                details: Some(if mcp_registered {
                    "registered".into()
                } else {
                    "not registered".into()
                }),
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
                details: Some(if ollama_running {
                    "available".into()
                } else {
                    "not detected".into()
                }),
            },
        }
    }
}
