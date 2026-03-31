# CLAUDE.md

## Project Overview

Memoryport is a Rust workspace that gives LLM interactions persistent, queryable memory using Arweave for permanent storage and LanceDB for local vector indexing. Data on Arweave is encrypted (AES-256-GCM) with per-batch keys and an Argon2id-derived master key. Includes a React dashboard (Tauri desktop + web), transparent API proxy for Anthropic/OpenAI/Ollama, and MCP server for Claude Code/Cursor.

**Website:** [memoryport.ai](https://memoryport.ai) — Next.js app in separate repo (`t8/memoryport-web`)
**AMP Spec:** [github.com/t8/amp-spec](https://github.com/t8/amp-spec) — open protocol for memory injection

## Build & Test

```bash
cargo check                # type-check entire workspace
cargo build                # build all crates
cargo test --workspace     # run all tests (167+ tests)
cargo build -p uc-cli      # CLI only
cargo build -p uc-mcp      # MCP server only
cargo build -p uc-proxy    # API proxy only
cargo build -p uc-server   # hosted API server only
cd ui && pnpm dev          # React dashboard dev server
```

### Dev Script (recommended)

```bash
./dev.sh start    # Build and start server + proxy + UI
./dev.sh stop     # Stop all services
./dev.sh restart  # Rebuild and restart everything
./dev.sh status   # Show what's running
./dev.sh server   # Rebuild and restart just the server
./dev.sh proxy    # Rebuild and restart just the proxy
./dev.sh ui       # Restart just the UI
./dev.sh logs proxy  # Tail proxy logs
```

### Tauri Desktop App

```bash
cd crates/uc-tauri/src-tauri && cargo tauri dev    # Dev mode (needs UI dev server running)
cd crates/uc-tauri/src-tauri && cargo tauri build   # Production .dmg/.app
```

### Prerequisites

- Rust stable (1.91+)
- `protoc` (Protocol Buffers compiler) — required by LanceDB's lance-encoding
  - macOS: `brew install protobuf`
- Node.js 18+ and pnpm (for UI)

### Binaries

| Binary | Crate | Purpose |
|--------|-------|---------|
| `uc` | uc-cli | CLI: init, store, query, retrieve, delete, rebuild-index, proxy, status, flush |
| `uc-mcp` | uc-mcp | MCP server (stdio) for Claude Code / Cursor |
| `uc-proxy` | uc-proxy | Multi-protocol API proxy (Anthropic + OpenAI + Ollama) |
| `uc-server` | uc-server | Multi-tenant hosted API server with auth, metrics, dashboard |

### Running the Stack Locally

```bash
# 1. Start the API server (port 8090 if Open WebUI uses 8080)
UC_SERVER_LISTEN=127.0.0.1:8090 ./target/debug/uc-server --config ~/.memoryport/uc.toml

# 2. Start the proxy (port 9191)
./target/debug/uc-proxy --config ~/.memoryport/uc.toml --listen 127.0.0.1:9191

# 3. Start the React dashboard (port 5174, proxies API to 8090)
cd ui && pnpm dev
```

## Architecture

9 crates in the workspace:

```
uc-arweave     — Arweave client (JWK wallet, ANS-104 data items, Turbo uploads, GraphQL)
uc-embeddings  — Embedding + LLM providers (OpenAI, Ollama) for embeddings and query enhancement
uc-core        — Core engine: chunker, tagger, batcher, writer, LanceDB index (chunks + facts tables),
                 retriever (hybrid vector+BM25+entity with RRF fusion), reranker, assembler,
                 analyzer, enhancer (query expansion + HyDE), facts (NLP extraction),
                 entities (Jaro-Winkler dedup), profile (user profile cache),
                 contradiction (fact superseding), crypto, keystore,
                 gate (3-gate retrieval gating), analytics, rebuild
uc-cli         — CLI binary with interactive setup wizard (`uc init`)
uc-mcp         — MCP server: 7 tools + 2 resources, auto-capture via uc_auto_store
uc-proxy       — Multi-protocol proxy: Anthropic /v1/messages, OpenAI /v1/chat/completions,
                 Ollama /api/chat + /api/generate. Context injection + auto-capture.
uc-server      — Multi-tenant HTTP API: per-user engine pool, API key auth, rate limiting,
                 Prometheus metrics, integration management, serves React dashboard
uc-tauri       — Tauri 2.x desktop app with sidecar binaries, lazy engine init, service manager,
                 auto-updater (checks GitHub releases for latest.json)
```

Frontend: `ui/` — React 19 + Vite + Tailwind. Dashboard, analytics, integrations, settings.

**Dependency graph:** `uc-arweave` and `uc-embeddings` have no internal deps. `uc-core` depends on both. Everything else depends on `uc-core`.

## Release & Distribution

### CLI Install
- **Canonical URL:** `curl -fsSL https://memoryport.ai/install | sh` (served by memoryport-web `src/app/install/route.ts`)
- **Repo script:** `install.sh` (installs to `~/.memoryport/bin`)
- Both install all 4 binaries: `uc`, `uc-mcp`, `uc-proxy`, `uc-server`

### Desktop App
- Built by `.github/workflows/tauri-build.yml` on version tags (`v*`)
- Platforms: macOS (aarch64, x86_64), Linux (x86_64). Windows disabled for now.
- macOS builds are code-signed and notarized (Apple Developer ID: Not Community Labs Inc)
- Download redirect API: `memoryport.ai/api/download?platform=mac|linux`

### Auto-Updater
- Plugin: `tauri-plugin-updater` v2 in `crates/uc-tauri/src-tauri/`
- Endpoint: `https://github.com/t8/memoryport/releases/latest/download/latest.json`
- Signing key: private key stored as `TAURI_SIGNING_PRIVATE_KEY` GitHub secret, public key in `tauri.conf.json`
- CI generates `latest.json` manifest from `.sig` files during release

### CI Workflows
- `tauri-build.yml` — Tauri desktop app builds + `latest.json` updater manifest
- `release.yml` — CLI binary tarballs (`memoryport-{os}-{arch}.tar.gz`)
- Both trigger on `v*` tags

## Terminology

- **Context window**: tokens the LLM sees in one call (model constraint, e.g. 200K)
- **Context space**: total persistent memory available for retrieval and injection (system property, measured in tokens/chunks/bytes)
- **Chunk**: ~1,500 characters ≈ **375 tokens** (with 200-char overlap between adjacent chunks). Character-based splitting at sentence boundaries.
- **Fact**: atomic subject-predicate-object triple extracted from a chunk (e.g., "user lives_in Austin"). Stored in a separate LanceDB table with its own embedding, dual timestamps, and validity tracking.
- **Entity**: deduplicated named entity (person, place, project, tool) with aliases, tracked via Jaro-Winkler similarity for fuzzy matching.
- **Batch**: group of up to 50 chunks flushed together as one Arweave transaction
- **AMP**: Augmented Memory Protocol — the [open spec](https://github.com/t8/amp-spec) for how memory enters the prompt
- **RRF**: Reciprocal Rank Fusion — algorithm for merging multiple ranked result sets (chunk search + fact search)

## Key Technical Decisions

- **ANS-104 from scratch** — deep hash (SHA-384), Avro tag encoding, RSA-PSS signing.
- **LanceDB v0.15** with arrow-array/arrow-schema v53 — versions must stay aligned.
- **`checkout_latest()`** before every LanceDB read — required for cross-process visibility (proxy writes, server reads).
- **`count_rows()`** for counting — query-based counting returns stale cached results.
- **Explicit `.limit()` on all LanceDB queries** — Rust API defaults to 10 rows without it.
- **Compaction, not IVF-PQ** — LanceDB fragment compaction brought 2s queries to 104ms. IVF-PQ was slower and took 30+ minutes to build.
- **AES-256-GCM encryption** — per-batch keys wrapped with Argon2id master key.
- **Three-gate retrieval gating** — rule-based → embedding routing → post-retrieval quality check.
- **Proxy uses raw `serde_json::Value`** — never deserialize/reserialize Anthropic content blocks (causes "Input tag 'Other'" errors with tool_use, thinking, etc.).
- **Context injection as plain text** — appended to the last user message. XML tags and system prompt injection are filtered by Claude Code. Separate fake user/assistant pairs are ignored.
- **SSE streaming response parsing** — proxy accumulates `content_block_delta` events for Anthropic and NDJSON lines for Ollama.
- **Content sanitization** — strip `<system-reminder>`, `<local-command-*>`, Open WebUI title/tag generation, Claude Code memory file dumps before storage and retrieval.
- **Open WebUI meta-request detection** — skip title/tag/emoji generation requests (contain "### Task:", "JSON format", etc.) for both injection and capture.
- **Source tagging** — every chunk tagged with `source_integration` (proxy, proxy-ollama, mcp, cli, api) and `source_model` (claude-opus-4-20250514, llama3.2:1b, etc.).
- **API key + Turbo credit sharing** — Pro users get a `uc_` API key from memoryport.ai. Local client validates against `/api/validate`, auto-generates an Arweave wallet, uploads with `x-paid-by` header for Turbo credit sharing.
- **AccountClient** — `uc-core/src/account.rs` handles key validation (1-hour cache), usage reporting, graceful fallback to local-only on network errors.
- **Hot-reloadable proxy config** — `HotConfig` in proxy checks config file mtime on each request. Settings changes (e.g., multi-turn toggle) take effect without proxy restart.
- **Augmented Memory Protocol (AMP)** — the context injection approach is being standardized as [AMP](https://github.com/t8/amp-spec), defining how memory enters the prompt (append, system, tool strategies).
- **Hybrid retrieval with RRF** — parallel vector search on chunks + vector search on facts, merged via Reciprocal Rank Fusion (k=60). Facts provide high-precision atomic matches; chunks provide full conversation context.
- **NLP-based fact extraction (no LLM required)** — regex patterns extract preferences, personal info, temporal statements, project references from conversation text. Optional LLM mode for higher quality. Runs at ingestion time, not query time.
- **Dual timestamps on facts** — `document_date` (when conversation happened) + `event_date` (when the fact became true). Enables temporal queries like "what changed" and "when did X happen".
- **Contradiction resolution at ingestion** — when a new fact has the same subject + conflicting predicate as an existing fact (e.g., lives_in NYC → lives_in London), the old fact is marked `valid=false` with `superseded_by` pointing to the new fact. Old facts are never deleted — they're preserved for temporal history.
- **Session diversity** — hybrid retrieval caps results at 5 chunks per session to ensure broad coverage across sessions. Prevents a single semantically dominant session from filling all result slots.
- **`reference_time` parameter** — retrieve/query APIs accept an optional reference timestamp. The analyzer computes temporal ranges relative to this time instead of `now()`. Used by benchmarks to evaluate with historical data, and available for production queries about past conversations.
- **User profile cache** — `ProfileManager` maintains static facts (name, role, location, preferences) and dynamic facts (current projects, recent topics) in a JSON file at `~/.memoryport/profile-{user_id}.json`. Updated incrementally on each store operation. Designed for 0ms injection into retrieval responses.
- **Entity registry with Jaro-Winkler dedup** — extracted entities are deduplicated using three tiers: exact case-insensitive match → Jaro-Winkler similarity > 0.85 → create new. Aliases are accumulated across matches.

## UI Design System

- **Palette:** Dark bg (#0d0d0d), cream text (#fff4e0), cream-muted (0.7 opacity), cream-dim (0.5 opacity), accent green (#84cc16), error red
- **Fonts:** Upheaval TT BRK (display/logo), Switzer (sans body), DM Mono (monospace)
- **Components:** StatusCard (with tooltip, suffix), Toggle, Tooltip, SearchBar, SessionList
- **Icons:** Lucide icons with `text-cream-dim` class. Ollama uses custom SVG at `ui/public/integrations/ollama.svg`. Arweave uses `ⓐ` glyph.

## Conventions

- Error types: `thiserror` derive macros, one error enum per module
- Async runtime: `tokio` everywhere
- Config: TOML files parsed with `serde` + `toml` crate
- Arweave tags: constants in `uc-core/src/tagger.rs` (`APP_NAME`, `APP_VERSION`, `SCHEMA_VERSION`)
- Default paths: `~/.memoryport/` for user config/data
- GitHub: `t8/memoryport` — never use any other username
- Settings/toggles must actually persist and take effect, not just update UI state
- Never add Co-Authored-By or Claude attribution to commits

## Learnings

- `rmcp` 1.2 API: use `#[tool_router(router = tool_router)]` on impl block, `#[tool_handler(router = self.tool_router)]` on `ServerHandler` impl. Resources need `Annotated::new(resource, None)` wrapper. `ServerInfo` is non-exhaustive — use `ServerInfo::new(capabilities)`. Use `ErrorData` not deprecated `Error`.
- `rsa` crate PSS signing: use `BlindedSigningKey::<Sha256>`, get bytes via `SignatureEncoding::to_vec()`.
- LanceDB `FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>` for vector columns. `Array` trait must be imported for `is_null()`.
- LanceDB cross-process reads: `Table` caches version snapshots. Must call `checkout_latest()` before reads, or use `count_rows()` instead of query-based counting.
- LanceDB Rust API defaults to 10 rows on `query()` — always use explicit `.limit()`.
- LanceDB compaction (`OptimizeAction::Compact`) is the fix for fragmentation. NOT `All` (which rebuilds indices) and NOT IVF-PQ (slower for our scale).
- `tower-http` `TimeoutLayer::with_status_code(status, duration)` — status code is the first argument.
- Argon2id salt must be at least 16 bytes.
- ANS-104 deep hash: tags must be raw avro bytes (`encode_avro_tags`), not structured tag pairs.
- Claude Code MCP config goes in `~/.claude.json` (not `~/.claude/settings.json`).
- Claude Code sends `authorization: Bearer` header, not `x-api-key`. Proxy must forward both.
- Claude Code system prompt content leaks into stored chunks via the proxy. Must sanitize `<system-reminder>`, memory file dumps, and `<local-command-*>` tags before storage.
- Anthropic content blocks have many types (tool_use, thinking, etc.). Never deserialize into typed enums with `#[serde(other)]` — the "Other" variant serializes as invalid. Use raw `serde_json::Value` for passthrough.
- Open WebUI sends meta-requests through the same `/api/chat` endpoint (title generation, tag generation). Detect by checking for "### Task:", "JSON format" in system prompts.
- Ollama desktop app doesn't support custom endpoints or MCP. Only terminal, Open WebUI, Continue.dev, and API clients can route through the proxy.
- Ollama.app on macOS auto-respawns via launchd. Don't try to kill it and take its port.
- Tauri updater needs minisign key pair. Generate with `cargo tauri signer generate --ci`. Private key goes in `TAURI_SIGNING_PRIVATE_KEY` GitHub secret, public key in `tauri.conf.json`.
- Tauri updater `latest.json` must be generated in CI from `.sig` files and uploaded to GitHub release.
- SVG icons with `fill="currentColor"` don't work in `<img>` tags — bake the fill color into the SVG file and use CSS opacity to adjust.
