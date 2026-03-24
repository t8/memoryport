# Unlimited Context

Persistent, queryable memory for LLMs — powered by [Arweave](https://arweave.org) and [LanceDB](https://lancedb.com).

Every conversation turn, uploaded document, and knowledge artifact is stored permanently on Arweave and made retrievable through a high-performance local vector index. The system selectively hydrates an LLM's context window with the most relevant stored data before each inference call, effectively removing the context window size as a constraint.

## How It Works

```
User message → Chunk → Embed → Store on Arweave + Index in LanceDB
                                        ↓
Query → Embed → Vector Search → Rerank → Assemble Context → LLM
```

- **Write path:** Text is chunked, tagged with Arweave metadata, batched, embedded, uploaded to Arweave via [ar.io Turbo](https://ar.io), and indexed locally in LanceDB.
- **Read path:** Queries are embedded and matched against the local index via vector similarity search with metadata filtering. Results are ranked and assembled into a context payload for the LLM.
- **Arweave = source of truth.** The local LanceDB index can be fully rebuilt from Arweave at any time (cold start).

## Quick Start

### Prerequisites

- Rust 1.91+ (`rustup update stable`)
- `protoc` (`brew install protobuf` on macOS)

### Build

```bash
cargo build -p uc-cli
```

### Usage

```bash
# Store text
uc store "Arweave uses a one-time payment model for permanent storage" \
  -u user_123 -s session_abc -t knowledge

# Query by semantic similarity
uc query "How does Arweave pricing work?" -u user_123 -k 5

# Check status
uc status

# Flush buffered chunks
uc flush
```

### Configuration

Create a `uc.toml` file (or use defaults):

```toml
[arweave]
gateway = "https://arweave.net"
turbo_endpoint = "https://upload.ardrive.io"
# wallet_path = "~/.unlimited-context/wallet.json"

[index]
path = "~/.unlimited-context/index"
embedding_dimensions = 1536

[embeddings]
provider = "openai"           # or "ollama"
model = "text-embedding-3-small"
dimensions = 1536
# api_key = "sk-..."          # or set OPENAI_API_KEY env var

# For Ollama:
# provider = "ollama"
# model = "nomic-embed-text"
# dimensions = 768
# api_base = "http://localhost:11434"
```

## Integration Interfaces

| Interface | Status | Description |
|-----------|--------|-------------|
| CLI | Done | `uc store`, `uc query`, `uc status`, `uc flush` |
| Rust SDK | Done | `uc-core::Engine` — `store()`, `query()`, `flush()`, `status()` |
| MCP Server | Planned | Tools: `uc_store`, `uc_query`, `uc_get_session`, `uc_list_sessions` |
| API Proxy | Planned | OpenAI-compatible middleware with automatic context injection |
| TypeScript SDK | Planned | `@unlimited-context/sdk` via napi-rs |

## Architecture

```
crates/
├── uc-arweave/      # Arweave client (wallet, ANS-104, Turbo, GraphQL)
├── uc-embeddings/   # Embedding providers (OpenAI, Ollama)
├── uc-core/         # Core engine (chunker, batcher, index, retrieval)
├── uc-cli/          # CLI binary
├── uc-mcp/          # MCP server (planned)
└── uc-proxy/        # API proxy (planned)
```

## Cost

Arweave storage is permanent and pay-once (~$7/GB as of March 2026):

| Usage | Size | Cost |
|-------|------|------|
| 1 conversation turn | ~1 KB | $0.000007 |
| 1 month heavy usage | ~15 MB | $0.11 |
| 1 year heavy usage | ~180 MB | $1.26 |
| Power user: 5 years | ~1 GB | $7.00 |

Chunks under 100 KiB are **free** via ar.io Turbo.

## License

MIT OR Apache-2.0
