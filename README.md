```
 __  __                                                _
|  \/  | ___ _ __ ___   ___  _ __ _   _ _ __   ___  _ __| |_
| |\/| |/ _ \ '_ ` _ \ / _ \| '__| | | | '_ \ / _ \| '__| __|
| |  | |  __/ | | | | | (_) | |  | |_| | |_) | (_) | |  | |_
|_|  |_|\___|_| |_| |_|\___/|_|   \__, | .__/ \___/|_|   \__|
                                   |___/|_|
   Destroyer of the context window
```

Memoryport gives LLMs persistent, queryable memory using [Arweave](https://arweave.org) for permanent storage and [LanceDB](https://lancedb.com) for local vector search. Every conversation, document, and knowledge artifact is stored permanently and retrieved semantically — so your AI never forgets.

Works with **Claude Code**, **Cursor**, **Open WebUI**, **Ollama**, and any OpenAI-compatible tool.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/t8/memoryport/main/install.sh | sh
```

Then run the setup wizard:
```bash
uc init
```

That's it. Restart your editor — Memoryport auto-captures conversations and surfaces relevant context.

Also available via:
```bash
brew install memoryport/tap/uc     # Homebrew
npx @memoryport/cli init           # npm
```

### Build from Source

```bash
# Prerequisites: Rust 1.91+, protoc (brew install protobuf), Node.js 18+
cargo build --release
cd ui && pnpm install && pnpm build  # Dashboard
```

## How It Works

```
User sends a message (Claude Code, Open WebUI, Cursor, terminal, API)
  │
  ▼
Memoryport proxy intercepts transparently
  │
  ├─ Search stored memory for relevant context
  │   ├─ Gate 1: Skip greetings/commands (0ms)
  │   ├─ Gate 2: Embedding routing (0ms marginal)
  │   └─ Gate 3: Drop low-quality results
  │
  ├─ Inject relevant context into the message
  │
  ├─ Forward to LLM (Anthropic, OpenAI, Ollama)
  │
  ├─ Capture both user message + assistant response
  │   ├─ Sanitize (strip system prompts, internal commands)
  │   ├─ Embed via configured provider
  │   └─ Store in LanceDB + optionally sync to Arweave
  │
  └─ Return response to user
```

## Supported Integrations

| Tool | Method | Setup |
|------|--------|-------|
| **Claude Code** | API Proxy | `uc init` configures automatically (sets `ANTHROPIC_BASE_URL`) |
| **Cursor** | API Proxy | Set `ANTHROPIC_BASE_URL=http://127.0.0.1:9191` |
| **Open WebUI** | Ollama Proxy | Set Ollama URL to `http://127.0.0.1:9191` in Settings → Connections |
| **Ollama (terminal)** | Ollama Proxy | `OLLAMA_HOST=http://127.0.0.1:9191 ollama run llama3` |
| **Continue.dev** | Ollama/OpenAI Proxy | Set endpoint to `http://127.0.0.1:9191` |
| **Any OpenAI SDK app** | API Proxy | `OPENAI_BASE_URL=http://127.0.0.1:9191` |
| **Claude Code (MCP)** | MCP Server | `uc init` registers MCP automatically |
| **Cursor (MCP)** | MCP Server | `uc init` registers MCP automatically |

The proxy handles all three API formats on a single port (9191):
- **Anthropic** `/v1/messages`
- **OpenAI** `/v1/chat/completions`
- **Ollama** `/api/chat`, `/api/generate`, `/api/tags`, and all `/api/*` routes

## Dashboard

Memoryport includes a React dashboard for visualizing your stored memories:

```bash
# Start the API server + proxy + dashboard
uc-server --config ~/.memoryport/uc.toml    # API on :8090
uc-proxy --config ~/.memoryport/uc.toml     # Proxy on :9191
cd ui && pnpm dev                            # Dashboard on :5174
```

**Pages:**
- **Dashboard** — status cards, session browser, semantic search with keyword highlighting
- **Analytics** — activity sparklines, storage growth, type/source distribution, memory density heatmap, sync status
- **Integrations** — toggle MCP server, API proxy, Ollama capture on/off. Real controls that write config and start/stop services.
- **Settings** — embedding provider, model, API key, smart gating, encryption, Arweave wallet

Also available as a Tauri desktop app (macOS/Windows/Linux).

## CLI

```bash
uc init                  # Interactive setup wizard
uc store "text" -t knowledge  # Store a chunk
uc query "search term"   # Full retrieval pipeline (gated + reranked + assembled)
uc retrieve "search"     # Raw vector search (bypasses gating)
uc proxy                 # Start the API proxy
uc delete --tx-id <id>   # Logical deletion (destroy encryption key)
uc rebuild-index -u <id> # Rebuild index from Arweave
uc status                # Index stats
uc flush                 # Flush pending writes
```

## MCP Tools

| Tool | Description |
|------|-------------|
| `uc_auto_store` | Silently store a conversation turn (called automatically) |
| `uc_store` | Store text with explicit metadata |
| `uc_query` | Semantic search with full retrieval pipeline |
| `uc_retrieve` | Raw ranked results |
| `uc_get_session` | Full conversation history for a session |
| `uc_list_sessions` | List all stored sessions |
| `uc_status` | System status |

## Configuration

`~/.memoryport/uc.toml` (created by `uc init`):

```toml
[arweave]
gateway = "https://arweave.net"
turbo_endpoint = "https://upload.ardrive.io"
# wallet_path = "~/.memoryport/wallet.json"

[index]
path = "~/.memoryport/index"
embedding_dimensions = 768

[embeddings]
provider = "ollama"              # or "openai"
model = "nomic-embed-text"
dimensions = 768

[retrieval]
max_context_tokens = 50000
similarity_top_k = 50
recency_window = 20
gating_enabled = true            # Three-gate system: skip greetings, route by embedding, filter low quality
# query_expansion = true         # LLM generates alternative search terms
# hyde = true                    # Embed hypothetical answer instead of raw query
# llm_model = "gpt-4o-mini"

[encryption]
# enabled = true
# passphrase_env = "UC_MASTER_PASSPHRASE"

[proxy]
listen = "127.0.0.1:9191"
```

## Architecture

```
crates/
├── uc-arweave/      # Arweave client (wallet, ANS-104, Turbo, GraphQL)
├── uc-embeddings/   # Embedding + LLM providers (OpenAI, Ollama)
├── uc-core/         # Core engine (chunk, index, retrieve, rerank, assemble, encrypt, gate)
├── uc-cli/          # CLI binary with setup wizard
├── uc-mcp/          # MCP server (stdio, 7 tools, 2 resources)
├── uc-proxy/        # Multi-protocol API proxy (Anthropic + OpenAI + Ollama)
├── uc-server/       # Multi-tenant hosted API server + dashboard
└── uc-tauri/        # Tauri desktop app

ui/                  # React dashboard (Vite + Tailwind)
```

## Security

- All data on Arweave is encrypted with AES-256-GCM (per-batch random keys)
- Master key derived from passphrase via Argon2id
- Logical deletion: destroy batch key → ciphertext permanently unreadable
- API keys: 128-bit entropy, SHA-256 hashed, stored in SQLite
- Proxy sanitizes system prompts, internal commands, and meta-requests before storage

## Deployment

### Docker

```bash
docker compose up
```

Environment variables:
- `OPENAI_API_KEY` — for embeddings (if using OpenAI)
- `UC_ADMIN_API_KEY` — admin API key for user management
- `UC_SERVER_LISTEN` — listen address (default `0.0.0.0:8080`)
- `UC_SERVER_DATA_DIR` — data directory (default `/var/lib/uc-server`)

### Hosted API

```bash
# Create a user
curl -X POST http://localhost:8080/admin/users \
  -H "Authorization: Bearer $UC_ADMIN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"email": "user@example.com"}'
# Returns: { "user_id": "...", "api_key": "uc_..." }

# Store context
curl -X POST http://localhost:8080/v1/store \
  -H "Authorization: Bearer uc_..." \
  -H "Content-Type: application/json" \
  -d '{"text": "Arweave uses pay-once permanent storage", "chunk_type": "knowledge"}'

# Query
curl -X POST http://localhost:8080/v1/query \
  -H "Authorization: Bearer uc_..." \
  -H "Content-Type: application/json" \
  -d '{"query": "How does Arweave pricing work?"}'
```

## Cost

Arweave storage is permanent and pay-once (~$7/GB):

| Usage | Size | Cost |
|-------|------|------|
| 1 conversation turn | ~1 KB | $0.000007 |
| 1 month heavy usage | ~15 MB | $0.11 |
| 1 year heavy usage | ~180 MB | $1.26 |
| Power user: 5 years | ~1 GB | $7.00 |

Chunks under 100 KiB are **free** via ar.io Turbo.

Without Arweave configured, memories are stored locally only (free, no limit).

## License

MIT OR Apache-2.0
