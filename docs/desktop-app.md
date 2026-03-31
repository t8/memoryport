# Memoryport Desktop App

## Overview

The Memoryport desktop app is a native application built with Tauri 2.x that provides a visual interface for managing persistent LLM memory. It bundles the same core engine, proxy, and MCP server available through the CLI into a single installable package with a React-based dashboard.

Compared to the CLI, the desktop app provides:

- **Setup wizard** -- guided first-run configuration for embeddings, integrations, and cloud backup
- **Visual dashboard** -- search memory, browse sessions, view analytics and knowledge graphs
- **Service management** -- automatic lifecycle management of the uc-server, uc-proxy, and uc-mcp sidecar binaries with health monitoring and restart controls
- **Integration toggles** -- one-click enable/disable for MCP (Claude Code, Cursor) and the auto-capture proxy
- **Auto-updater** -- checks GitHub releases for new versions and applies updates in-place

## Installation

### Download

Download the latest release from [memoryport.ai](https://memoryport.ai) or from [GitHub Releases](https://github.com/t8/memoryport/releases).

Available formats:

| Platform | Format | Architecture |
|----------|--------|--------------|
| macOS | `.dmg` | Apple Silicon (aarch64), Intel (x86_64) |
| Linux | `.AppImage`, `.deb` | x86_64 |

Windows builds are planned but not yet included in the release pipeline.

### macOS Signing and Notarization

The macOS builds are code-signed with a Developer ID certificate from Not Community Labs Inc (team ID `W7P3D4QPYL`) and notarized through Apple's notary service. This means macOS will not show an "unidentified developer" warning when you open the app.

If you see a Gatekeeper warning on an older build, right-click the app and select "Open" to bypass it.

### Supported Platforms

- **macOS** 12+ (Monterey and later), both Apple Silicon and Intel
- **Linux** x86_64 with WebKit2GTK 4.1

### Prerequisites

The app bundles its own backend binaries, so you do not need Rust or Cargo installed. However, you do need:

- An embedding provider -- either an **OpenAI API key** or **Ollama** installed locally (the setup wizard can install Ollama for you)
- `protoc` is only needed if building from source

## Setup Wizard

On first launch, the app detects that no configuration exists at `~/.memoryport/uc.toml` and redirects to the setup wizard. The wizard has four steps:

### Step 1: Embedding Provider

Choose how text is converted to vectors for similarity search:

- **OpenAI** -- Uses `text-embedding-3-small` (1536 dimensions). Requires an OpenAI API key (`sk-...`). If the `OPENAI_API_KEY` environment variable is set, you can leave the field blank.
- **Ollama** -- Uses `nomic-embed-text` (768 dimensions). Free and fully local. The wizard checks if Ollama is installed, installs it via `curl -fsSL https://ollama.com/install.sh | sh` if needed (macOS/Linux), and pulls the embedding model automatically.

### Step 2: Integrations

Enable integrations so your AI tools can access Memoryport:

- **MCP Server** -- Registers `memoryport` as an MCP server in Claude Code (`~/.claude.json`), Claude Desktop (`~/Library/Application Support/Claude/claude_desktop_config.json`), and Cursor (`~/.cursor/mcp.json`). Provides memory as tools (store, retrieve, query) to these editors.
- **Auto-Capture Proxy** -- Sets `ANTHROPIC_BASE_URL=http://127.0.0.1:9191` in `~/.claude.json` so all Claude API traffic passes through the Memoryport proxy. The proxy transparently captures conversations and injects relevant memory into context.

Both integrations require restarting your editor/tool after enabling. You can skip this step and configure integrations later from the Integrations page.

### Step 3: Cloud Backup (Optional)

Enter a Memoryport Pro API key (`uc_...`) from [memoryport.ai/dashboard](https://memoryport.ai/dashboard) to enable permanent backup of your memory to Arweave. Data is encrypted with AES-256-GCM before upload.

You can skip this step for local-only operation with no network dependency.

### Step 4: Launch

The final step shows a summary of your configuration and a "Launch Memoryport" button that:

1. Writes `~/.memoryport/uc.toml` with your settings
2. Creates the `~/.memoryport/index/` directory for the LanceDB vector index
3. Initializes the core engine (loads config, opens the index)
4. Starts sidecar services (uc-server on port 8090, uc-proxy on port 9191 if configured)
5. Polls service health until the engine reports "running" (up to 15 seconds)
6. Redirects to the main dashboard

If any step fails, the wizard shows the error with an option to view technical details and retry from the point of failure.

## Dashboard Pages

After setup, the app shows a sidebar with five pages, plus a service status panel and version indicator.

### Dashboard

The main landing page. Provides:

- **Status cards** -- pending chunks (not yet flushed), indexed chunks in the vector database, embedding model and dimensions
- **Search** -- vector similarity search across all stored memory. Results show chunk content, session ID, relevance score, timestamp, source integration, and source model.
- **Session browser** -- list of all conversation sessions with chunk counts and time ranges. Click a session to view its full content with role labels (user/assistant), source integration, and model tags.

### Analytics

Lazy-loaded page with D3-based charts:

- **Activity chart** -- daily chunk ingestion counts over time
- **Content distribution** -- breakdown of chunks by type (conversation, document, etc.)
- **Source breakdown** -- chunks by source integration (proxy, MCP, CLI, API) and by model
- **Sync status** -- ratio of locally-indexed vs. Arweave-synced chunks
- **Totals** -- total chunk and session counts

### Graph

Lazy-loaded page with a D3 force-directed graph visualization:

- Nodes represent conversation sessions, sized by chunk count
- Edges represent semantic similarity between sessions, weighted by relatedness
- Interactive -- drag nodes, zoom, pan

### Integrations

Toggle integrations on and off with live status indicators:

- **MCP Server** -- shows "registered" or "not registered". Enabling writes the MCP config to all supported editor config files. Disabling removes it.
- **Auto-Capture Proxy** -- shows "configured" or "not configured". Enabling starts the proxy process, sets `ANTHROPIC_BASE_URL` in `~/.claude.json`. Disabling kills the proxy and restores the original base URL.
- **Ollama** -- shows whether Ollama is reachable on port 11434 and whether the proxy is configured to capture Ollama traffic.
- **Arweave** -- shows whether a wallet file exists at `~/.memoryport/wallet.json`.

### Settings

Persistent settings that write directly to `~/.memoryport/uc.toml`:

- **Embeddings** -- provider (OpenAI/Ollama), model, dimensions, API key, custom API base URL
- **Retrieval** -- gating enabled (three-gate retrieval quality filter), similarity top-k, recency window
- **Proxy** -- agentic mode (multi-turn context injection), capture-only mode (store conversations without injecting memory)
- **Arweave** -- gateway URL, wallet path, API key (validated against memoryport.ai), storage usage stats, wallet address, import/export wallet
- **Encryption** -- toggle AES-256-GCM encryption, set/change passphrase

API keys are masked as `--------` in the UI and only overwritten when a new non-masked value is provided.

Additional actions on the Settings page:

- **Sync to Arweave** -- manually flush pending chunks to permanent storage
- **Rebuild from Arweave** -- restore the local index from Arweave transactions (with progress tracking)
- **Reset all data** -- stops services, shuts down the engine, unregisters the proxy and MCP, and deletes the entire `~/.memoryport/` directory

## Service Management

The desktop app manages three sidecar binaries that are bundled inside the application package:

| Binary | Default Port | Purpose |
|--------|-------------|---------|
| `uc-server` | 8090 | API server -- handles all data operations (status, sessions, search, analytics, settings) |
| `uc-proxy` | 9191 | Transparent API proxy -- intercepts Anthropic/OpenAI/Ollama traffic for auto-capture and context injection |
| `uc-mcp` | stdio | MCP server -- registered with Claude Code, Claude Desktop, and Cursor for tool-based memory access |

### Lifecycle

- **On launch**: if a config file exists, the app registers the MCP server in all editor configs, then starts `uc-server` and `uc-proxy` (if the proxy is configured). It polls health endpoints (`/health` on each port) for up to 20 seconds.
- **On close**: the app stops all managed processes, restores `ANTHROPIC_BASE_URL` in `~/.claude.json` to its original value, and removes the `memoryport` MCP server entry from all editor configs. This ensures your editors work normally when the desktop app is not running.
- **kill_on_drop**: all child processes are spawned with Tokio's `kill_on_drop(true)`, so if the app crashes, the sidecar processes are cleaned up automatically.

### Binary Discovery

The service manager searches for sidecar binaries in this order:

1. Same directory as the running executable (production -- Tauri bundles sidecars here)
2. System PATH (development -- picks up `cargo build` output)
3. `~/.memoryport/bin/` (manual installation)

### Status Indicators

The sidebar shows a "Services" panel with real-time status for four services:

- **Engine** (port 8090) -- the uc-server process
- **Proxy** (port 9191) -- the uc-proxy process
- **MCP** -- whether the MCP server is registered in editor configs (checked structurally, not by string matching)
- **Ollama** (port 11434) -- whether Ollama is reachable and the proxy is configured to capture its traffic

Each service shows a colored dot:

| Color | Status |
|-------|--------|
| Green | Running |
| Yellow | Starting / Unhealthy |
| Red | Crashed |
| Gray | Stopped |

Click a service row to expand details (uptime, restart count, port). The Engine and Proxy services have a "Restart" button. Global "Restart All" and "Stop All" controls are at the bottom of the panel.

Health checks run periodically and hit the `/health` endpoint on each service's port.

## Auto-Updater

The app uses the Tauri updater plugin to check for and apply updates.

### How It Works

1. On launch, the app fetches `https://github.com/t8/memoryport/releases/latest/download/latest.json`
2. This JSON file contains the latest version number and platform-specific download URLs with signatures
3. If a newer version is available, an update badge appears next to the version number in the sidebar (e.g., "0.2.0 available")
4. Clicking the badge downloads the update and verifies its signature against the public key embedded in `tauri.conf.json`
5. After successful download and verification, the app relaunches automatically

### Update States

- **Idle** -- no update available or check has not run
- **Available** -- a new version badge is shown, clickable to start the update
- **Downloading** -- shows a spinner with "Updating..."
- **Ready** -- update applied, "Restarting..." shown briefly before relaunch
- **Error** -- update failed, reverts to "available" state after 3 seconds for retry

### Signing

Updates are signed with a minisign private key during the CI build. The corresponding public key is hardcoded in `tauri.conf.json`. The updater will reject any update that does not match this signature.

## Development

### Prerequisites

- Rust stable (1.91+)
- `protoc` -- `brew install protobuf` on macOS
- Node.js 18+ and pnpm
- Tauri CLI -- `cargo install tauri-cli --version "^2"`

### Dev Mode

Run the React dev server and Tauri app together:

```bash
# Terminal 1: Start the UI dev server (hot-reloading on port 5174)
cd ui && pnpm dev

# Terminal 2: Start the Tauri app in dev mode
cd crates/uc-tauri/src-tauri && cargo tauri dev
```

In dev mode, Tauri loads the UI from `http://localhost:5174` (the Vite dev server) instead of the bundled static files, so you get hot module replacement for the frontend.

### Production Build

```bash
# Build the full .dmg / .AppImage / .msi
cd crates/uc-tauri/src-tauri && cargo tauri build
```

The build process:

1. Compiles the three sidecar binaries (`uc-proxy`, `uc-mcp`, `uc-server`) for the target platform
2. Copies them to `crates/uc-tauri/src-tauri/binaries/` with architecture suffixes
3. Builds the React UI with `pnpm build` (output to `ui/dist/`)
4. Compiles the Tauri app, bundling the UI and sidecars into a platform installer

### CI/CD

The GitHub Actions workflow at `.github/workflows/tauri-build.yml` automates the release process:

- Triggered on `v*` tags
- Builds for macOS aarch64, macOS x86_64, and Linux x86_64
- Signs macOS builds with the Developer ID certificate
- Notarizes macOS `.dmg` files through Apple's notary service
- Signs updater artifacts with the Tauri signing key
- Generates `latest.json` for the auto-updater
- Creates a GitHub release with all artifacts

## Architecture

### Tauri 2.x

The app is built on Tauri 2.x with these plugins:

- `tauri-plugin-shell` -- execute shell commands (used for Ollama installation)
- `tauri-plugin-updater` -- auto-update support with signature verification
- `tauri-plugin-process` -- app relaunch after updates

The Rust backend directly depends on `uc-core` and `uc-arweave`, so the in-process engine has full access to the vector index, chunker, embeddings, and Arweave client without needing HTTP round-trips.

### Sidecar Binaries

Three external binaries are bundled with the app and managed as child processes:

```
Memoryport.app/
  Contents/
    MacOS/
      Memoryport          # Main Tauri binary (Rust + WebView)
      uc-server-*         # API server sidecar
      uc-proxy-*          # API proxy sidecar
      uc-mcp-*            # MCP server sidecar
```

The `ServiceManager` in `services.rs` handles spawning, health checking, and killing these processes. All are started with `kill_on_drop(true)` so they terminate if the parent process exits.

### IPC Commands

The Tauri app exposes 28 IPC commands that the frontend invokes via `@tauri-apps/api/core`:

**Data commands:**
`get_status`, `list_sessions`, `get_session`, `retrieve`, `store_text`, `get_graph`, `get_analytics`

**Integration commands:**
`get_integrations`, `toggle_integration`

**Settings commands:**
`get_settings`, `update_settings`

**Setup and lifecycle commands:**
`check_config_exists`, `write_initial_config`, `init_engine`, `get_service_health`, `start_services`, `stop_services`, `restart_service`, `check_ollama_installed`, `install_ollama`, `pull_ollama_model`, `register_mcp`, `register_proxy`, `unregister_proxy`, `rebuild_from_arweave`, `get_operation_progress`, `sync_to_arweave`, `reset_all_data`, `validate_api_key`, `import_wallet`, `export_wallet`

### Dual-Mode API

The React frontend works in two modes with a single codebase:

- **Desktop mode** (Tauri) -- detects `window.__TAURI_INTERNALS__` and routes all API calls through `@tauri-apps/api/core.invoke()`, which uses Tauri's IPC bridge to call Rust functions directly
- **Web mode** (browser) -- falls back to standard HTTP `fetch()` calls against the uc-server REST API (e.g., `GET /v1/status`, `POST /v1/retrieve`)

The abstraction layer is in `ui/src/lib/api.ts`. Every API function checks `IS_TAURI` and branches accordingly:

```typescript
export async function getStatus(): Promise<Status> {
  if (IS_TAURI) return tauriInvoke("get_status");
  return httpGet("/v1/status");
}
```

This means the same React app serves as both the Tauri desktop UI and the web dashboard served by uc-server.

### State Management

The Tauri backend manages five pieces of shared state, all stored as Tauri managed state:

| State | Type | Purpose |
|-------|------|---------|
| `AppEngine` | `Arc<RwLock<Option<Arc<Engine>>>>` | The core engine instance, `None` until setup completes |
| `AppRuntime` | `tokio::runtime::Runtime` | Dedicated Tokio runtime for async operations |
| `AppConfigPath` | `PathBuf` | Path to `~/.memoryport/uc.toml` |
| `AppServices` | `Arc<RwLock<Option<ServiceManager>>>` | Manages sidecar process lifecycle |
| `AppProgress` | `Arc<RwLock<Option<RebuildProgress>>>` | Progress tracking for long-running operations |

### Window Configuration

- Default size: 1200x800 pixels
- Minimum size: 800x600 pixels
- CSP: disabled (null) to allow loading local resources
- Identifier: `com.memoryport.app`
