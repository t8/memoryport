# CLAUDE.md

## Project Overview

Unlimited Context is a Rust workspace that gives LLM interactions persistent, queryable memory using Arweave for permanent storage and LanceDB for local vector indexing.

## Build & Test

```bash
cargo check          # type-check entire workspace
cargo build          # build all crates
cargo build -p uc-cli # build CLI binary only
cargo test           # run all tests
cargo test -p uc-core # run tests for a specific crate
./target/debug/uc --help # run CLI
```

### Prerequisites

- Rust stable (1.91+)
- `protoc` (Protocol Buffers compiler) — required by LanceDB's lance-encoding dependency
  - macOS: `brew install protobuf`

## Architecture

Rust workspace with 6 crates:

```
uc-arweave     — Arweave client (wallet, ANS-104 data items, Turbo uploads, GraphQL)
uc-embeddings  — Embedding provider abstraction (OpenAI, Ollama)
uc-core        — Core engine (chunker, tagger, batcher, writer, LanceDB index, Engine facade)
uc-cli         — CLI binary (`uc store`, `uc query`, `uc status`, `uc flush`)
uc-mcp         — MCP server (stub, Phase 3)
uc-proxy       — OpenAI-compatible API proxy (stub, Phase 3)
```

**Dependency graph:** `uc-arweave` and `uc-embeddings` have no internal deps. `uc-core` depends on both. `uc-cli`, `uc-mcp`, `uc-proxy` depend on `uc-core`.

## Key Technical Decisions

- **ANS-104 implemented from scratch** — deep hash (SHA-384), Avro tag encoding, RSA-PSS signing. No dependency on `bundles-rs` for full control over the binary format.
- **RSA-PSS signing** via `rsa` crate's `BlindedSigningKey<Sha256>` — Arweave signature type 1.
- **LanceDB v0.15** with arrow-array/arrow-schema v53 — versions must stay aligned. Arrow version mismatches cause type incompatibility at compile time.
- **`async-trait`** for `EmbeddingProvider` — needed because native async fn in traits doesn't support `dyn Trait`.
- **Batcher uses `Arc<Mutex<Vec<Chunk>>>`** — simpler than channels for push/flush semantics with low contention.

## Conventions

- Error types: `thiserror` derive macros, one error enum per module.
- Async runtime: `tokio` everywhere.
- Config: TOML files parsed with `serde` + `toml` crate.
- Arweave tags: constants in `uc-core/src/tagger.rs` (`APP_NAME`, `APP_VERSION`, `SCHEMA_VERSION`).

## Learnings

<!-- Future agents: add learnings here as you work on the project -->
