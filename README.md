```
 __  __                                                _
|  \/  | ___ _ __ ___   ___  _ __ _   _ _ __   ___  _ __| |_
| |\/| |/ _ \ '_ ` _ \ / _ \| '__| | | | '_ \ / _ \| '__| __|
| |  | |  __/ | | | | | (_) | |  | |_| | |_) | (_) | |  | |_
|_|  |_|\___|_| |_| |_|\___/|_|   \__, | .__/ \___/|_|   \__|
                                   |___/|_|
   Permanent memory for LLMs. Store once, recall forever.
```

Memoryport gives LLMs persistent, queryable memory using [Arweave](https://arweave.org) for permanent storage and [LanceDB](https://lancedb.com) for local vector search. Every conversation, document, and knowledge artifact is stored permanently and retrieved semantically — so your AI never forgets.

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

## Quick Start

```bash
curl -fsSL https://raw.githubusercontent.com/t8/memoryport/main/install.sh | sh
```

Or with Homebrew:
```bash
brew install memoryport/tap/uc
```

Or with npm:
```bash
npx @memoryport/cli init
```

### Setup

```bash
uc init
```

The interactive wizard will:
1. Ask you to choose an embedding provider (OpenAI or Ollama)
2. Auto-install Ollama if you choose it
3. Create config at `~/.memoryport/uc.toml`
4. Register the MCP server in Claude Code / Cursor

After setup, restart your editor. Memoryport is active — it auto-captures conversations and surfaces relevant context.

### Build from Source

```bash
# Prerequisites: Rust 1.91+, protoc (brew install protobuf)
cargo build --release
```

## How It Works

```
User message
  → (optional) LLM expands query + generates hypothetical answer (HyDE)
  → Embed → Vector search across ALL stored history
  → Recency window + session affinity + temporal/explicit reference detection
  → Rerank (recency decay, session boost, MMR diversity)
  → Assemble XML context within token budget
  → Inject into LLM call
  → Store conversation turn → Arweave (permanent, encrypted) + LanceDB (searchable)
```

## Integration Interfaces

| Interface | Binary | Description |
|-----------|--------|-------------|
| CLI | `uc` | init, store, query, retrieve, delete, rebuild-index, status, flush |
| MCP Server | `uc-mcp` | 7 tools + 2 resources over stdio — auto-capture + auto-context |
| API Proxy | `uc-proxy` | OpenAI-compatible proxy with automatic context injection |
| Hosted API | `uc-server` | Multi-tenant REST API with auth, rate limiting, Prometheus metrics |
| Rust SDK | — | `uc-core::Engine` — `store()`, `query()`, `retrieve()`, `flush()`, `delete_batch()` |

### MCP Tools

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
embedding_dimensions = 1536

[embeddings]
provider = "openai"              # or "ollama"
model = "text-embedding-3-small"
dimensions = 1536

[retrieval]
max_context_tokens = 50000
similarity_top_k = 50
recency_window = 20
# query_expansion = true         # LLM generates alternative search terms
# hyde = true                     # embed hypothetical answer instead of raw query
# llm_model = "gpt-4o-mini"

[encryption]
# enabled = true
# passphrase_env = "UC_MASTER_PASSPHRASE"
```

## Architecture

```
crates/
├── uc-arweave/      # Arweave client (wallet, ANS-104, Turbo, GraphQL)
├── uc-embeddings/   # Embedding + LLM providers (OpenAI, Ollama)
├── uc-core/         # Core engine (chunk, index, retrieve, rerank, assemble, encrypt)
├── uc-cli/          # CLI binary with setup wizard
├── uc-mcp/          # MCP server (stdio, 7 tools, 2 resources)
├── uc-proxy/        # OpenAI-compatible API proxy
└── uc-server/       # Multi-tenant hosted API server
```

## Security

- All data on Arweave is encrypted with AES-256-GCM (per-batch random keys)
- Master key derived from passphrase via Argon2id
- Logical deletion: destroy batch key → ciphertext permanently unreadable
- API keys: 128-bit entropy, SHA-256 hashed, stored in SQLite

## Deployment

### Docker

```bash
docker compose up
```

Environment variables:
- `OPENAI_API_KEY` — for embeddings
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

## License

MIT OR Apache-2.0
