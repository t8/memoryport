# CLAUDE.md

## Project Overview

Memoryport is a Rust workspace that gives LLM interactions persistent, queryable memory using Arweave for permanent storage and LanceDB for local vector indexing. Data on Arweave is encrypted (AES-256-GCM) with per-batch keys and an Argon2id-derived master key.

## Build & Test

```bash
cargo check              # type-check entire workspace
cargo build              # build all crates
cargo test               # run all tests (48 tests)
cargo build -p uc-cli    # CLI only
cargo build -p uc-mcp    # MCP server only
cargo build -p uc-proxy  # OpenAI proxy only
cargo build -p uc-server # hosted API server only
```

### Prerequisites

- Rust stable (1.91+)
- `protoc` (Protocol Buffers compiler) — required by LanceDB's lance-encoding
  - macOS: `brew install protobuf`

### Binaries

| Binary | Crate | Purpose |
|--------|-------|---------|
| `uc` | uc-cli | CLI: init, store, query, retrieve, delete, rebuild-index, status, flush |
| `uc-mcp` | uc-mcp | MCP server (stdio) for Claude Code / Cursor |
| `uc-proxy` | uc-proxy | OpenAI-compatible API proxy with context injection |
| `uc-server` | uc-server | Multi-tenant hosted API server with auth + metrics |

## Architecture

8 crates in the workspace:

```
uc-arweave     — Arweave client (JWK wallet, ANS-104 data items, Turbo uploads, GraphQL)
uc-embeddings  — Embedding + LLM providers (OpenAI, Ollama) for embeddings and query enhancement
uc-core        — Core engine: chunker, tagger, batcher, writer, LanceDB index, retriever,
                 reranker, assembler, analyzer, enhancer (query expansion + HyDE), crypto,
                 keystore, rebuild
uc-cli         — CLI binary with interactive setup wizard (`uc init`)
uc-mcp         — MCP server: 7 tools + 2 resources, auto-capture via uc_auto_store
uc-proxy       — OpenAI proxy: intercepts /v1/chat/completions, injects context
uc-server      — Multi-tenant HTTP API: per-user engine pool, API key auth, rate limiting,
                 Prometheus metrics, health/readiness probes
```

**Dependency graph:** `uc-arweave` and `uc-embeddings` have no internal deps. `uc-core` depends on both. Everything else depends on `uc-core`.

## Key Technical Decisions

- **ANS-104 from scratch** — deep hash (SHA-384), Avro tag encoding, RSA-PSS signing. Full control over binary format.
- **LanceDB v0.15** with arrow-array/arrow-schema v53 — versions must stay aligned.
- **AES-256-GCM encryption** — per-batch keys wrapped with Argon2id master key. Logical deletion by key destruction.
- **`async-trait`** for `EmbeddingProvider` and `LlmProvider` — needed for `dyn Trait`.
- **Query expansion + HyDE** — LLM generates alternative phrasings and hypothetical answers for better vector search recall.
- **Heuristic reranker** — recency decay, session affinity, MMR diversity. `Reranker` trait for swappable implementations.
- **Per-user Engine pool** (uc-server) — `RwLock<HashMap>` with LRU eviction. Each user gets isolated LanceDB.
- **SHA-256 for API key hashing** — 128-bit entropy keys, industry standard (Stripe/GitHub pattern).
- **rmcp 1.2** for MCP server — `#[tool_router]` macros, stdio transport.
- **axum 0.8** for HTTP servers (proxy + server).

## Conventions

- Error types: `thiserror` derive macros, one error enum per module
- Async runtime: `tokio` everywhere
- Config: TOML files parsed with `serde` + `toml` crate
- Arweave tags: constants in `uc-core/src/tagger.rs` (`APP_NAME`, `APP_VERSION`, `SCHEMA_VERSION`)
- Default paths: `~/.memoryport/` for user config/data
- GitHub: `t8/memoryport` — never use any other username

## Learnings

- `rmcp` 1.2 API: use `#[tool_router(router = tool_router)]` on impl block, `#[tool_handler(router = self.tool_router)]` on `ServerHandler` impl. Resources need `Annotated::new(resource, None)` wrapper. `ServerInfo` is non-exhaustive — use `ServerInfo::new(capabilities)`.
- `rsa` crate PSS signing: use `BlindedSigningKey::<Sha256>`, get bytes via `SignatureEncoding::to_vec()`.
- LanceDB `FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>` for vector columns. `Array` trait must be imported for `is_null()`.
- `tower-http` `TimeoutLayer::with_status_code(status, duration)` — status code is the first argument.
- Argon2id salt must be at least 16 bytes.
