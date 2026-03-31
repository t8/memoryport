# Memoryport CLI Reference

The `uc` command-line tool provides direct access to Memoryport's memory engine: storing text, searching memory, managing the index, and running the API proxy.

## Installation

### Quick Install

```bash
curl -fsSL https://memoryport.ai/install | sh
```

This installs the `uc`, `uc-proxy`, and `uc-mcp` binaries.

### Build from Source

Prerequisites: Rust 1.91+, `protoc` (Protocol Buffers compiler).

```bash
# macOS: install protoc
brew install protobuf

# Build all binaries
cargo build --release

# Binaries are in target/release/
ls target/release/uc target/release/uc-proxy target/release/uc-mcp
```

## Setup Wizard

Run `uc init` to configure Memoryport interactively. The wizard walks through six steps:

```bash
uc init
```

### Step 1: Choose Embedding Provider

Select between OpenAI (cloud, requires API key) and Ollama (local, free, private).

- **OpenAI**: Uses `text-embedding-3-small` (1536 dimensions). If `OPENAI_API_KEY` is already in your environment, the wizard detects it and offers to reuse it. Otherwise, you can enter a key or set the environment variable later.
- **Ollama**: Uses `nomic-embed-text` (768 dimensions). The wizard checks if Ollama is installed, offers to install it if not, and pulls the embedding model.

### Step 2: Cloud Storage

Optionally enter a Memoryport Pro API key (`uc_...`) for permanent Arweave backup. Press Enter to skip and use local-only mode. You can add a key later in the config file or dashboard.

### Step 3: Write Configuration

Creates `~/.memoryport/uc.toml` and the `~/.memoryport/index/` directory. If a config file already exists, the wizard asks before overwriting.

### Step 4: Register MCP Server

Registers the `uc-mcp` binary in:

- **Claude Code**: Adds an entry to `~/.claude.json` under `mcpServers.memoryport`
- **Cursor**: Adds an entry to `~/.cursor/mcp.json` under `mcpServers.memoryport` (if the `.cursor` directory exists)

The MCP entry points to the `uc-mcp` binary with `--config ~/.memoryport/uc.toml`.

### Step 5: Auto-capture Proxy

If you choose to enable the proxy:

- Appends a `[proxy]` section to `uc.toml` with `listen = "127.0.0.1:9191"`
- Sets `ANTHROPIC_BASE_URL=http://127.0.0.1:9191` in `~/.claude.json` (if the file exists) so Claude Code routes through the proxy

### Step 6: Summary

Displays the final configuration and next steps. If the proxy was enabled, it tells you how to start it.

## Commands

All commands accept a `--config` / `-c` flag to specify the config file path (default: `uc.toml`).

```bash
uc --config ~/.memoryport/uc.toml <command>
```

### init

Interactive setup wizard. Does not require an existing config file or engine.

```bash
uc init
```

### store

Store text content in memory.

```bash
uc store <text> [options]
```

**Arguments:**

| Argument | Description |
|----------|-------------|
| `text` | The text content to store (required, positional) |

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--user-id` | `-u` | `default` | User identifier. Resolves to the engine's configured user ID when set to `default`. |
| `--session-id` | `-s` | `default` | Session identifier for grouping related chunks. |
| `--chunk-type` | `-t` | `conversation` | Content type: `conversation`, `document`, or `knowledge`. |
| `--role` | `-r` | none | Role of the author: `user`, `assistant`, or `system`. |

**Examples:**

```bash
# Store a knowledge fact
uc store "Arweave uses pay-once permanent storage" -t knowledge

# Store a conversation turn with role
uc store "The deployment uses Docker Compose" -t conversation -r assistant

# Store with explicit session
uc store "Working on the auth refactor" -s project-auth -r user
```

The command automatically flushes after storing and prints the chunk IDs:

```
Stored 1 chunk(s):
  abc123def456
```

### query

Search memory using the full retrieval pipeline: gating, vector search, reranking, and context assembly.

```bash
uc query <text> [options]
```

**Arguments:**

| Argument | Description |
|----------|-------------|
| `text` | The search query (required, positional) |

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--user-id` | `-u` | `default` | User identifier. |
| `--session-id` | `-s` | none | Active session ID for recency and session affinity scoring. |
| `--max-tokens` | `-m` | `50000` | Maximum tokens in assembled context output. |

**Examples:**

```bash
# Search for relevant context
uc query "How does the auth system work?"

# Search with a smaller context budget
uc query "What did I say about Docker?" -m 10000

# Search within a specific session's context
uc query "deployment steps" -s project-deploy
```

Output shows the assembled context with chunk and token counts:

```
--- Assembled Context (5 chunks, ~2340 tokens) ---

[context content here]
```

### retrieve

Raw vector search that bypasses gating and context assembly. Returns individual ranked results with scores and metadata.

```bash
uc retrieve <text> [options]
```

**Arguments:**

| Argument | Description |
|----------|-------------|
| `text` | The search query (required, positional) |

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--user-id` | `-u` | `default` | User identifier. |
| `--session-id` | `-s` | none | Active session ID. |
| `--top-k` | `-k` | `10` | Number of results to display. |

**Examples:**

```bash
# Get top 5 raw results
uc retrieve "Arweave pricing" -k 5

# Full detail on search results
uc retrieve "authentication flow"
```

Output shows each result with score, chunk ID, session, type, role, timestamp, and a 200-character content preview:

```
--- Result 1 (score: 0.8234) ---
  Chunk:   abc123
  Session: proxy-20250315-141020
  Type:    conversation
  Role:    assistant
  Time:    2025-03-15T14:10:20Z
  Content: Arweave uses a pay-once model where you pay a single...
```

### proxy

Start the auto-capture proxy. This is a convenience wrapper that finds and launches the `uc-proxy` binary.

```bash
uc proxy
```

The proxy listens on the address specified in `[proxy] listen` (default `127.0.0.1:8080`, typically configured as `127.0.0.1:9191` by `uc init`).

The proxy handles three API formats on a single port:

- **Anthropic** `/v1/messages`
- **OpenAI** `/v1/chat/completions`
- **Ollama** `/api/chat`, `/api/generate`, `/api/tags`, and all `/api/*` routes

### status

Show engine status: pending chunks, indexed chunks, index path, and embedding model.

```bash
uc status
```

Output:

```
Pending chunks:      0
Indexed chunks:      4521
Index path:          /Users/you/.memoryport/index
Embedding model:     text-embedding-3-small (1536d)
```

### flush

Flush any buffered chunks to the index immediately. Chunks are normally batched (up to 50 per batch) and flushed automatically, but this forces an immediate write.

```bash
uc flush
```

### delete

Logically delete a batch by destroying its encryption key. The data remains on Arweave but becomes permanently unreadable.

```bash
uc delete --tx-id <arweave-transaction-id>
```

**Options:**

| Flag | Description |
|------|-------------|
| `--tx-id` | Arweave transaction ID of the batch to delete (required). |

**Example:**

```bash
uc delete --tx-id abc123xyz789
```

Output:

```
Batch key destroyed for tx abc123xyz789. Data is now permanently unreadable.
```

If no batch key exists for the given transaction:

```
No batch key found for tx abc123xyz789.
```

### rebuild-index

Rebuild the local LanceDB index by downloading and re-indexing all data from Arweave. Use this when setting up on a new machine or recovering from data loss.

```bash
uc rebuild-index -u <user-id>
```

**Options:**

| Flag | Short | Description |
|------|-------|-------------|
| `--user-id` | `-u` | User ID to rebuild for (required). |

**Example:**

```bash
uc rebuild-index -u default
```

Output:

```
Rebuilding index for user 'default' from Arweave...
Rebuild complete:
  Transactions found:     47
  Transactions processed: 47
  Chunks indexed:         2350
  Errors:                 0
```

## Configuration File

Memoryport uses TOML configuration at `~/.memoryport/uc.toml`. The `uc init` wizard creates this file. Below is a complete reference with all sections and options.

### Full Example

```toml
[arweave]
gateway = "https://arweave.net"
turbo_endpoint = "https://upload.ardrive.io"
wallet_path = "~/.memoryport/wallet.json"
api_key = "uc_your_api_key_here"
api_endpoint = "https://memoryport.ai/api"
enabled = true

[index]
path = "~/.memoryport/index"
embedding_dimensions = 1536

[embeddings]
provider = "openai"
model = "text-embedding-3-small"
dimensions = 1536
api_key = "sk-..."
# api_base = "https://custom-endpoint.example.com"

[retrieval]
max_context_tokens = 50000
recency_window = 20
similarity_top_k = 50
gating_enabled = true
min_relevance_score = 0.3
# rerank = true
# query_expansion = true
# hyde = true
# llm_provider = "openai"
# llm_model = "gpt-4o-mini"

[proxy]
listen = "127.0.0.1:9191"
# upstream = "https://api.anthropic.com"
# capture_only = false

[proxy.agentic]
enabled = true
max_rounds = 3

[encryption]
enabled = false
passphrase_env = "UC_MASTER_PASSPHRASE"
# passphrase = "your-passphrase-here"
```

### Section Reference

#### `[arweave]`

| Key | Default | Description |
|-----|---------|-------------|
| `gateway` | `https://arweave.net` | Arweave gateway URL for reading data. |
| `turbo_endpoint` | `https://upload.ardrive.io` | Turbo upload endpoint for writing data. |
| `wallet_path` | none | Path to Arweave JWK wallet file. Auto-generated when an API key is configured. |
| `api_key` | none | Memoryport Pro API key (`uc_...`). Also read from `UC_API_KEY` env var. |
| `api_endpoint` | `https://memoryport.ai/api` | API endpoint for key validation and usage reporting. |
| `enabled` | `false` | Enable Arweave permanent backup. Requires a valid Pro API key. |

#### `[index]`

| Key | Default | Description |
|-----|---------|-------------|
| `path` | `~/.memoryport/index` | Path to the LanceDB index directory. Tilde (`~`) is expanded. |
| `embedding_dimensions` | `1536` | Vector dimensions. Must match the embedding model (1536 for OpenAI, 768 for nomic-embed-text). |

#### `[embeddings]`

| Key | Default | Description |
|-----|---------|-------------|
| `provider` | `openai` | Embedding provider: `openai` or `ollama`. |
| `model` | `text-embedding-3-small` | Embedding model name. |
| `dimensions` | `1536` | Vector dimensions produced by the model. |
| `api_key` | none | API key for the provider. Not required for Ollama. Also read from `OPENAI_API_KEY` env var for OpenAI. |
| `api_base` | none | Custom API base URL. Use for OpenAI-compatible endpoints or custom Ollama hosts. |

#### `[retrieval]`

| Key | Default | Description |
|-----|---------|-------------|
| `max_context_tokens` | `50000` | Maximum tokens in assembled context output. |
| `recency_window` | `20` | Number of recent chunks to include for recency scoring. |
| `similarity_top_k` | `50` | Number of candidates to retrieve from vector search before reranking. |
| `rerank` | `false` | Enable cross-encoder reranking of results. |
| `query_expansion` | `false` | Use an LLM to generate alternative search terms. |
| `hyde` | `false` | Hypothetical Document Embeddings: embed a hypothetical answer instead of the raw query. |
| `llm_provider` | none | LLM provider for query expansion and HyDE: `openai` or `ollama`. |
| `llm_model` | none | LLM model for query expansion and HyDE (e.g., `gpt-4o-mini`). |
| `gating_enabled` | `true` | Enable three-gate retrieval gating (skip retrieval for greetings, commands, short queries). |
| `min_relevance_score` | `0.3` | Minimum relevance score to include a result (Gate 3). Results below this threshold are dropped. |

#### `[proxy]`

| Key | Default | Description |
|-----|---------|-------------|
| `listen` | `127.0.0.1:8080` | Address and port for the proxy to listen on. Typically set to `127.0.0.1:9191` by `uc init`. |
| `upstream` | none | Override the default Anthropic upstream URL. If not set, the proxy uses `https://api.anthropic.com`. |
| `capture_only` | `false` | When true, the proxy captures conversations but does not inject memory context into requests. Useful when MCP handles injection. |

#### `[proxy.agentic]`

| Key | Default | Description |
|-----|---------|-------------|
| `enabled` | `true` | Enable multi-turn agentic retrieval. The proxy injects a memory search tool and lets the LLM query memory iteratively before responding. |
| `max_rounds` | `3` | Maximum tool-call rounds before the proxy stops the agentic loop and returns the response. |

#### `[encryption]`

| Key | Default | Description |
|-----|---------|-------------|
| `enabled` | `false` | Enable AES-256-GCM encryption for Arweave uploads. |
| `passphrase_env` | `UC_MASTER_PASSPHRASE` | Name of the environment variable containing the master passphrase. |
| `passphrase` | none | Master passphrase stored directly in config. Alternative to using an environment variable. |

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `OPENAI_API_KEY` | OpenAI API key for embeddings (when using OpenAI provider). Used if not set in config. |
| `UC_API_KEY` | Memoryport Pro API key. Used if not set in `[arweave] api_key`. |
| `ANTHROPIC_API_KEY` | Anthropic API key. Forwarded by the proxy to Anthropic's upstream. |
| `ANTHROPIC_BASE_URL` | Set to `http://127.0.0.1:9191` to route Claude Code through the proxy. Set automatically by `uc init`. |
| `UC_MASTER_PASSPHRASE` | Master passphrase for encryption (when using the default `passphrase_env`). |
| `RUST_LOG` | Logging level for tracing output (e.g., `RUST_LOG=debug`, `RUST_LOG=uc_proxy=debug`). |

## Common Workflows

### First-Time Setup

```bash
# 1. Install
curl -fsSL https://memoryport.ai/install | sh

# 2. Run the setup wizard
uc init

# 3. Start the proxy (if enabled during init)
uc-proxy --config ~/.memoryport/uc.toml

# 4. Restart your editor (Claude Code, Cursor)
# Memory capture and retrieval are now automatic.
```

### Daily Use

Once configured, Memoryport runs in the background. The proxy captures conversations automatically when your editor routes API calls through it. The MCP server starts automatically when your editor launches.

```bash
# Check how much memory is stored
uc --config ~/.memoryport/uc.toml status

# Search your memory manually
uc --config ~/.memoryport/uc.toml query "What was the auth approach we discussed?"

# Store a note directly
uc --config ~/.memoryport/uc.toml store "Deployment uses k8s with Helm charts" -t knowledge
```

### Rebuilding from Arweave (New Machine)

If you have a Memoryport Pro subscription and need to set up on a new machine:

```bash
# 1. Install and run init on the new machine
curl -fsSL https://memoryport.ai/install | sh
uc init

# 2. Enter your Pro API key during init (or add it to uc.toml)

# 3. Import your wallet keyfile (exported from the old machine)
# Place it at ~/.memoryport/wallet.json

# 4. Rebuild the index
uc --config ~/.memoryport/uc.toml rebuild-index -u default
```

The rebuild downloads all encrypted batches from Arweave, decrypts them with your passphrase, and re-indexes the chunks locally.

### Switching Embedding Providers

Changing the embedding model makes existing vectors incompatible with new ones. You need to rebuild from scratch:

```bash
# 1. Edit ~/.memoryport/uc.toml — change [embeddings] section
# 2. Delete the old index
rm -rf ~/.memoryport/index

# 3. If you have Arweave backup, rebuild with the new model
uc --config ~/.memoryport/uc.toml rebuild-index -u default

# 4. If local-only, you start fresh (old memories are lost)
```

### Using with Ollama

```bash
# Route Ollama terminal through the proxy
OLLAMA_HOST=http://127.0.0.1:9191 ollama run llama3

# Or configure Open WebUI to use the proxy
# Settings > Connections > Ollama URL: http://127.0.0.1:9191
```

### Running the Full Stack Manually

```bash
# Start the API server (for the dashboard)
UC_SERVER_LISTEN=127.0.0.1:8090 ./target/debug/uc-server --config ~/.memoryport/uc.toml

# Start the proxy
./target/debug/uc-proxy --config ~/.memoryport/uc.toml --listen 127.0.0.1:9191

# Start the React dashboard
cd ui && pnpm dev
```

Or use the dev script:

```bash
./dev.sh start    # Build and start server + proxy + UI
./dev.sh stop     # Stop all services
./dev.sh status   # Show what's running
./dev.sh logs proxy  # Tail proxy logs
```
