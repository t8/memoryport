# Unlimited Context: Technical Specification & Architecture Proposal

**Version:** 0.1.0-draft
**Date:** March 24, 2026
**Author:** Tate Berenbaum + Claude

---

## 1. Executive Summary

Unlimited Context is an open-core system that gives LLM interactions persistent, queryable, permanent memory by using Arweave as the storage backbone. Every conversation turn, uploaded document, and knowledge artifact is stored permanently on Arweave and made retrievable through a high-performance local index. The system selectively hydrates an LLM's context window with the most relevant stored data before each inference call, effectively removing the context window size as a constraint.

The system exposes itself as an MCP server, an OpenAI-compatible API middleware, and an importable SDK — making it compatible with Claude Code, Cursor, OpenClaw, and any LLM toolchain.

---

## 2. Problem Statement

LLM context windows are finite and ephemeral. Even with 200K+ token windows, users lose context across sessions, can't reference months of prior conversations, and have no persistent knowledge base that grows over time. Existing RAG solutions rely on centralized cloud storage (Pinecone, Weaviate, etc.) that introduces vendor lock-in, ongoing costs, and single points of failure.

Arweave solves the storage layer permanently: pay once, store forever, with cryptographic verifiability and decentralized redundancy. What's missing is the retrieval and context-assembly layer purpose-built for LLM consumption.

---

## 3. Design Principles

1. **Arweave-native.** No Irys, no AO, no Hyperbeam. Pure Arweave L1 transactions via ar.io infrastructure.
2. **Permanent by default.** All data written to Arweave is immutable and permanent. Logical deletion via encryption key destruction.
3. **Local-first retrieval.** The hot path never hits the network. Arweave is the source of truth; a local index handles all reads.
4. **Provider-agnostic.** Works with any LLM — Claude, GPT, local models via Ollama, or any OpenAI-compatible API.
5. **Open core.** Core engine is open-source (MIT/Apache 2.0). Hosted tier adds managed infrastructure and premium features.

---

## 4. Technology Choices

### 4.1 Language: Rust

**Rationale:** The workload is mixed — network I/O (Arweave transactions, gateway queries), vector math (embedding similarity over 1536-3072 dimensions), and JSON processing (Arweave transaction payloads, LLM API marshalling). Rust provides the best performance profile across all three:

- SIMD-accelerated vector operations (<1ms for similarity computation)
- `serde_json` at 500-1000 MB/s deserialization throughput
- `tokio` async runtime for high-concurrency network I/O
- Deterministic memory management (no GC pauses affecting tail latency)
- Native interop with LanceDB (also Rust-core)

TypeScript bindings will be provided via `napi-rs` for the SDK layer, ensuring compatibility with the Node.js/Deno ecosystem that most LLM toolchains run in.

### 4.2 Vector Database: LanceDB (Embedded)

**Rationale:** LanceDB is the best fit for an open-core product that needs to run embedded (in-process, no external server) while scaling to production workloads:

| Property | LanceDB |
|----------|---------|
| Architecture | Fully embedded, in-process |
| Query latency | 25ms vector search, ~50ms with metadata filtering |
| Scale | 200M+ vectors tested in production |
| Disk-based | Yes — not memory-bound, works on constrained hardware |
| Rust bindings | Native (it's written in Rust) |
| TypeScript bindings | Yes, via napi-rs |
| Metadata filtering | Full SQL-like filtering alongside vector similarity |
| Incremental updates | Supported with efficient indexing |
| Format | Lance columnar (Apache Arrow in-memory, DataFusion for queries) |

**Why not the alternatives:**

- **Qdrant:** Excellent but requires a separate server process. Adds deployment complexity for an open-source self-hosted product. Consider for a future distributed/hosted tier.
- **ChromaDB:** Not production-ready beyond ~50M vectors. Metadata filtering has performance overhead.
- **SQLite-vec:** No ANN indexing yet (brute-force only). Not viable for millions of vectors.
- **Turbopuffer:** Cloud-only, not self-hostable. Conflicts with open-core model.
- **Milvus Lite:** Python-only. No Rust or TypeScript bindings.

### 4.3 Arweave Infrastructure: ar.io

**Upload path:** ar.io Turbo for ANS-104 bundled uploads.

- Full ANS-104 bundling support (same spec Irys used)
- x402 JIT payments — pay per upload in USDC (Base), no balance management
- Free uploads under 100 KiB (covers most individual conversation turns)
- Multi-currency support (USDC, AR, ETH, SOL, POL)
- Irys SDK compatible (endpoint swap: `https://turbo.ardrive.io`)

**Retrieval path:** ar.io gateway network (public gateways for MVP, self-hosted for production).

- Self-hosted gateway: 4-core CPU, 4GB RAM minimum (12-core, 32GB recommended)
- Configurable indexing filters — index only your app's data, not all of Arweave
- Content caching for sub-second retrieval of hot data
- 10,000 ARIO stake required for self-hosted gateway operation

**Query path:** Arweave GraphQL via ar.io gateways for metadata queries and index rebuilds.

### 4.4 Embedding Model

The system will be embedding-model-agnostic, but will default to a high-quality open model for the self-hosted tier:

- **Default:** `nomic-embed-text-v1.5` (768 dimensions, Matryoshka support for dimension reduction, Apache 2.0 license)
- **Hosted tier options:** OpenAI `text-embedding-3-large` (3072d), Anthropic Voyage, Cohere Embed v3
- **Important:** Raw text is always stored on Arweave. Embeddings are computed at the index layer and stored locally in LanceDB. This avoids embedding model lock-in — switching models means reindexing, not re-uploading.

---

## 5. Architecture

### 5.1 High-Level Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        LLM Toolchain                            │
│         (Claude Code, Cursor, OpenClaw, custom apps)            │
└───────────┬──────────────────┬──────────────────┬───────────────┘
            │ MCP              │ API Middleware     │ SDK
            ▼                  ▼                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Unlimited Context Engine                      │
│                                                                 │
│  ┌──────────┐  ┌──────────────┐  ┌───────────────────────────┐ │
│  │ Context   │  │ Retrieval    │  │ Ingestion Pipeline        │ │
│  │ Assembler │◄─┤ Engine       │  │                           │ │
│  │           │  │              │  │  Chunker → Tagger →       │ │
│  │ Compress  │  │ Vector Search│  │  Batcher → Arweave Writer │ │
│  │ + Rank    │  │ + Metadata   │  │                           │ │
│  │ + Format  │  │ + Reranking  │  │                           │ │
│  └──────────┘  └──────┬───────┘  └─────────┬─────────────────┘ │
│                        │                     │                   │
│                ┌───────▼───────┐     ┌───────▼───────┐          │
│                │   LanceDB     │     │  ar.io Turbo  │          │
│                │ (Local Index) │     │  (Bundler)    │          │
│                └───────────────┘     └───────┬───────┘          │
│                        ▲                     │                   │
│                        │ Index Rebuild       ▼                   │
│                ┌───────┴───────┐     ┌───────────────┐          │
│                │  ar.io GQL    │◄────┤   Arweave L1  │          │
│                │  (Gateway)    │     │  (Permaweb)   │          │
│                └───────────────┘     └───────────────┘          │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 Data Flow: Write Path

```
User message / document upload
        │
        ▼
┌──────────────┐
│   Chunker    │  Split into semantic chunks (conversation turns,
│              │  document paragraphs, code blocks). Target: 256-512
│              │  tokens per chunk for optimal retrieval granularity.
└──────┬───────┘
       │
       ▼
┌──────────────┐
│   Tagger     │  Generate Arweave transaction tags:
│              │    - App-Name: "UnlimitedContext"
│              │    - App-Version: "0.1.0"
│              │    - Content-Type: "application/json"
│              │    - UC-User-Id: <user-id>
│              │    - UC-Session-Id: <session-id>
│              │    - UC-Chunk-Type: "conversation" | "document" | "knowledge"
│              │    - UC-Timestamp: <unix-ms>
│              │    - UC-Batch-Index: <index-within-batch>
│              │    - UC-Schema-Version: "1"
│              │  (Tag budget: 2048 bytes total. ~8-10 tags fit comfortably.)
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Local Buffer │  Accumulate chunks in memory/disk buffer.
│              │  Flush triggers:
│              │    - Buffer reaches 50 chunks
│              │    - 60 seconds since last flush
│              │    - Explicit flush (session end, user request)
│              │    - Single chunk > 100 KiB (immediate flush, still free via Turbo)
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Batch Writer │  Serialize buffered chunks into a single JSON payload:
│              │  {
│              │    "version": "1",
│              │    "chunks": [
│              │      {
│              │        "id": "<uuid>",
│              │        "type": "conversation",
│              │        "session_id": "...",
│              │        "timestamp": 1711324800000,
│              │        "role": "user" | "assistant",
│              │        "content": "...",
│              │        "metadata": { ... }
│              │      },
│              │      ...
│              │    ]
│              │  }
│              │
│              │  Submit as single Arweave transaction via ar.io Turbo.
│              │  Tags on the transaction describe the batch envelope.
│              │  Per-chunk metadata lives inside the JSON payload.
└──────┬───────┘
       │
       ├──► Arweave L1 (permanent storage, async confirmation)
       │
       ▼
┌──────────────┐
│ Index Writer │  Simultaneously compute embeddings for each chunk
│              │  and write to LanceDB:
│              │    - vector: float[768] (or model-specific dims)
│              │    - chunk_id: uuid
│              │    - arweave_tx_id: string
│              │    - batch_index: u32
│              │    - user_id: string
│              │    - session_id: string
│              │    - chunk_type: enum
│              │    - timestamp: i64
│              │    - content_preview: string (first 200 chars for display)
└──────────────┘
```

### 5.3 Data Flow: Read Path

```
LLM inference request (user sends a message)
        │
        ▼
┌──────────────┐
│  Query       │  Extract retrieval signals from the user's message:
│  Analyzer    │    - Semantic: embed the message for similarity search
│              │    - Temporal: detect time references ("last week", "earlier")
│              │    - Explicit: detect references ("that document", "session X")
│              │    - Recency: weight recent context higher by default
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  Retrieval   │  Multi-strategy retrieval from LanceDB:
│  Engine      │
│              │  1. Vector similarity search (top-K, K=50)
│              │     - Filtered by user_id (mandatory)
│              │     - Optionally filtered by session_id, chunk_type, time range
│              │
│              │  2. Recency window (last N chunks from current session)
│              │     - Always include recent context for conversational coherence
│              │     - Default: last 20 chunks from active session
│              │
│              │  3. Metadata-only queries (if explicit references detected)
│              │     - Direct lookup by session_id, document name, etc.
│              │
│              │  Merge results, deduplicate, produce candidate set.
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  Reranker    │  Score and rank candidates by relevance:
│              │    - Cross-encoder reranking (optional, adds ~50ms)
│              │    - Recency boost (exponential decay)
│              │    - Session affinity boost (same session = higher weight)
│              │    - Diversity penalty (avoid redundant chunks)
│              │  Select top-M chunks (M = budget / avg_chunk_tokens)
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  Context     │  Assemble the final context payload:
│  Assembler   │
│              │  1. Format chunks into a structured context block:
│              │     <unlimited_context>
│              │       <session id="abc" date="2026-03-20">
│              │         <turn role="user" time="14:30">...</turn>
│              │         <turn role="assistant" time="14:31">...</turn>
│              │       </session>
│              │       <document name="roadmap.md" stored="2026-03-15">
│              │         ...relevant excerpt...
│              │       </document>
│              │     </unlimited_context>
│              │
│              │  2. Apply context compression if budget is tight:
│              │     - Summarize older chunks (lossy but space-efficient)
│              │     - Truncate low-relevance chunks to key sentences
│              │
│              │  3. Prepend to user message or inject as system context
│              │     (configurable per integration)
└──────┬───────┘
       │
       ▼
    LLM API call with enriched context
```

### 5.4 Data Flow: Cold Start / Index Rebuild

When a user sets up on a new device or the local index is lost:

```
1. Query ar.io GraphQL gateway:
   transactions(tags: [
     { name: "App-Name", values: ["UnlimitedContext"] },
     { name: "UC-User-Id", values: ["<user-id>"] }
   ]) → returns all transaction IDs

2. Fetch transaction data from ar.io gateway for each TX ID.

3. Parse JSON payloads, extract chunks.

4. Compute embeddings for each chunk (batch processing).

5. Write to fresh LanceDB instance.

6. Index is now rebuilt from Arweave as source of truth.
```

**Optimization:** Store periodic index snapshots on Arweave (serialized LanceDB state). On cold start, load the latest snapshot and only replay transactions newer than the snapshot timestamp. This reduces rebuild from O(all_history) to O(recent_delta).

---

## 6. Integration Interfaces

### 6.1 MCP Server

Expose as a Model Context Protocol server for native integration with Claude Code, Cursor, Windsurf, and other MCP-compatible tools.

**Tools provided:**

| Tool | Description |
|------|-------------|
| `uc_store` | Store a chunk (text, document, knowledge) to Arweave + local index |
| `uc_query` | Semantic search across stored context |
| `uc_get_session` | Retrieve full conversation history for a session |
| `uc_list_sessions` | List all stored sessions with metadata |
| `uc_get_document` | Retrieve a stored document by name/ID |
| `uc_status` | System status: index size, pending writes, Arweave sync state |

**Resources provided:**

| Resource | Description |
|----------|-------------|
| `uc://context/auto` | Auto-retrieved context for the current conversation (injected into every request) |
| `uc://sessions/{id}` | Full session transcript |
| `uc://documents/{id}` | Stored document content |

### 6.2 OpenAI-Compatible API Middleware

A transparent proxy that sits between the client and any OpenAI-compatible LLM API:

```
Client → POST /v1/chat/completions → Unlimited Context Proxy
                                            │
                                            ├─ Extract last user message
                                            ├─ Run retrieval pipeline
                                            ├─ Inject context into messages array
                                            ├─ Forward to upstream LLM API
                                            ├─ Store response in write pipeline
                                            │
                                      ◄─────┘ Return LLM response to client
```

**Configuration:**
```toml
[proxy]
listen = "127.0.0.1:8080"
upstream = "https://api.anthropic.com"  # or any OpenAI-compatible endpoint

[retrieval]
max_context_tokens = 50000      # budget for injected context
recency_window = 20             # always include last N turns
similarity_top_k = 50           # candidate pool size
rerank = true                   # enable cross-encoder reranking

[arweave]
gateway = "https://arweave.net" # or self-hosted ar.io gateway
turbo_endpoint = "https://turbo.ardrive.io"
wallet_path = "~/.unlimited-context/wallet.json"

[index]
path = "~/.unlimited-context/index"
embedding_model = "nomic-embed-text-v1.5"
```

### 6.3 SDK (Rust + TypeScript)

**Rust crate:** `unlimited-context`

```rust
use unlimited_context::{Engine, Config, ChunkType};

let engine = Engine::new(Config::from_file("uc.toml")).await?;

// Store
engine.store("Hello, can you help me with my project?", ChunkType::Conversation {
    session_id: "session-123",
    role: Role::User,
}).await?;

// Retrieve
let context = engine.retrieve("What did we discuss about the project?", RetrievalOpts {
    max_chunks: 30,
    user_id: "user-456",
    ..Default::default()
}).await?;

// Get assembled context string
let formatted = context.format_for_llm(TokenBudget::new(50_000))?;
```

**TypeScript package:** `@unlimited-context/sdk` (via napi-rs wrapping the Rust core)

```typescript
import { Engine } from '@unlimited-context/sdk';

const engine = await Engine.create({ configPath: './uc.toml' });

await engine.store('Hello, can you help me?', {
  type: 'conversation',
  sessionId: 'session-123',
  role: 'user',
});

const context = await engine.retrieve('What did we discuss?', {
  maxChunks: 30,
  userId: 'user-456',
});
```

---

## 7. Data Model

### 7.1 Arweave Transaction Structure

**Transaction tags (envelope metadata, queryable via GraphQL):**

| Tag Name | Example Value | Purpose |
|----------|---------------|---------|
| `App-Name` | `UnlimitedContext` | App identifier for filtering |
| `App-Version` | `0.1.0` | Schema version for migrations |
| `Content-Type` | `application/json` | MIME type |
| `UC-User-Id` | `user_abc123` | Owner identifier |
| `UC-Session-Id` | `sess_xyz789` | Session grouping (optional for multi-session batches) |
| `UC-Chunk-Type` | `conversation\|document\|knowledge` | Content classification |
| `UC-Timestamp-Start` | `1711324800000` | Earliest chunk timestamp in batch |
| `UC-Timestamp-End` | `1711325400000` | Latest chunk timestamp in batch |
| `UC-Chunk-Count` | `12` | Number of chunks in this batch |
| `UC-Schema-Version` | `1` | Payload schema version |

**Transaction data payload:**

```json
{
  "schema_version": 1,
  "batch_id": "batch_unique_id",
  "chunks": [
    {
      "id": "chunk_uuid_1",
      "type": "conversation",
      "session_id": "sess_xyz789",
      "timestamp": 1711324800000,
      "role": "user",
      "content": "Can you help me understand how Arweave pricing works?",
      "metadata": {
        "token_count": 12,
        "language": "en"
      }
    },
    {
      "id": "chunk_uuid_2",
      "type": "conversation",
      "session_id": "sess_xyz789",
      "timestamp": 1711324830000,
      "role": "assistant",
      "content": "Arweave uses a one-time payment model for permanent storage...",
      "metadata": {
        "token_count": 145,
        "language": "en"
      }
    }
  ]
}
```

### 7.2 LanceDB Schema

```
Table: chunks
├── vector:        FixedSizeList[Float32, 768]   # embedding vector
├── chunk_id:      Utf8                           # unique chunk identifier
├── arweave_tx_id: Utf8                           # Arweave transaction containing this chunk
├── batch_index:   UInt32                         # position within the batch payload
├── user_id:       Utf8                           # owner
├── session_id:    Utf8                           # session grouping
├── chunk_type:    Utf8                           # "conversation" | "document" | "knowledge"
├── role:          Utf8                           # "user" | "assistant" | "system" | null
├── timestamp:     Int64                          # unix milliseconds
├── content:       Utf8                           # full text content (for reranking / display)
├── token_count:   UInt32                         # pre-computed token count
└── metadata:      Utf8                           # JSON blob for extensible metadata
```

**Indexes:**

- IVF-PQ vector index on `vector` column (auto-built by LanceDB)
- B-tree index on `user_id` (partition key for all queries)
- B-tree index on `timestamp` (for recency queries)
- B-tree index on `session_id` (for session-scoped retrieval)

---

## 8. Encryption & Logical Deletion

Since Arweave data is immutable, deletion requires an encryption-based approach:

### 8.1 Encryption Scheme

1. Each user has a **master key** derived from their wallet signature (or a separate passphrase).
2. Each batch transaction is encrypted with a unique **batch key** (AES-256-GCM).
3. The batch key is encrypted with the user's master key and stored as a tag or in a separate key-management transaction.
4. Data on Arweave is always ciphertext. Only the local engine (with the master key) can decrypt.

### 8.2 Logical Deletion

To "delete" data:

1. Destroy the batch key for the target transaction(s).
2. The ciphertext remains on Arweave but is permanently unreadable.
3. Remove corresponding entries from the local LanceDB index.
4. Optionally publish a "tombstone" transaction referencing the deleted TX IDs (for index rebuild awareness).

### 8.3 Selective Sharing

To share specific context with another user:

1. Re-encrypt the relevant batch key(s) with the recipient's public key.
2. Publish a "share grant" transaction on Arweave.
3. Recipient's engine discovers the grant via GraphQL and decrypts.

---

## 9. Cost Analysis

### 9.1 Storage Costs (at ~$7/GB, March 2026 estimate)

| Use Case | Data Size | One-Time Cost |
|----------|-----------|---------------|
| 1 conversation turn (~1 KB) | 1 KB | $0.000007 |
| 1 day heavy usage (~500 turns) | 500 KB | $0.0035 |
| 1 month heavy usage | 15 MB | $0.105 |
| 1 year heavy usage | 180 MB | $1.26 |
| 1,000 documents (avg 50 KB each) | 50 MB | $0.35 |
| Power user: 5 years of context | ~1 GB | $7.00 |

**Note:** Batching multiple chunks into single transactions further reduces overhead. Chunks under 100 KiB uploaded via Turbo are free.

### 9.2 Compute Costs (Self-Hosted)

| Component | Requirement | Est. Monthly Cost |
|-----------|-------------|-------------------|
| Embedding inference (local, CPU) | 4-core machine | ~$0 (runs on user's hardware) |
| Embedding inference (hosted, Nomic) | API calls | ~$0.01 per 1M tokens |
| LanceDB storage | Local disk | ~$0 (user's disk) |
| ar.io gateway (optional) | 4-core, 4GB RAM, 500GB SSD | ~$40/mo VPS + 10K ARIO stake |

### 9.3 Hosted Tier Costs (Projected)

For a managed SaaS offering:

| Tier | Included | Price |
|------|----------|-------|
| Free | 100 MB Arweave storage, 50K chunks indexed | $0 |
| Pro | 1 GB Arweave storage, 500K chunks, priority gateway | $10/mo |
| Team | 10 GB Arweave storage, 5M chunks, dedicated gateway, sharing | $50/mo |

---

## 10. Performance Targets

| Metric | Target | Mechanism |
|--------|--------|-----------|
| Write latency (local confirmation) | <10ms | Buffer to local disk, async Arweave submission |
| Write latency (Arweave confirmation) | <120s | Arweave L1 block time (~2 min) |
| Read latency (retrieval + assembly) | <100ms | LanceDB vector search (~25-50ms) + formatting (~10ms) |
| Cold start (index rebuild, 100K chunks) | <5 min | Parallel GraphQL pagination + batch embedding |
| Cold start (from snapshot, 100K chunks) | <30s | Load serialized LanceDB + replay delta |
| Index size (1M chunks, 768d embeddings) | ~6 GB disk | LanceDB disk-based storage |
| Concurrent users (hosted tier) | 1,000+ | Stateless retrieval engine, per-user LanceDB partitions |

---

## 11. Project Structure

```
unlimited-context/
├── Cargo.toml                    # Rust workspace root
├── crates/
│   ├── uc-core/                  # Core engine: chunking, tagging, batching, retrieval
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── chunker.rs        # Text chunking strategies
│   │   │   ├── tagger.rs         # Arweave tag generation
│   │   │   ├── batcher.rs        # Batch accumulation and flush logic
│   │   │   ├── writer.rs         # Arweave transaction submission via ar.io Turbo
│   │   │   ├── retriever.rs      # Multi-strategy retrieval from LanceDB
│   │   │   ├── reranker.rs       # Cross-encoder reranking
│   │   │   ├── assembler.rs      # Context formatting and compression
│   │   │   ├── index.rs          # LanceDB read/write operations
│   │   │   ├── crypto.rs         # Encryption, key management, logical deletion
│   │   │   ├── config.rs         # Configuration parsing
│   │   │   └── models.rs         # Core data types
│   │   └── Cargo.toml
│   │
│   ├── uc-arweave/               # Arweave client: transactions, GraphQL, gateway interaction
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── client.rs         # HTTP client for ar.io gateways
│   │   │   ├── turbo.rs          # ar.io Turbo upload client (ANS-104 bundling)
│   │   │   ├── graphql.rs        # GraphQL query builder and executor
│   │   │   ├── transaction.rs    # Transaction construction and signing
│   │   │   ├── wallet.rs         # Arweave wallet/key management
│   │   │   └── types.rs          # Arweave-specific types
│   │   └── Cargo.toml
│   │
│   ├── uc-embeddings/            # Embedding model abstraction
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── traits.rs         # EmbeddingProvider trait
│   │   │   ├── nomic.rs          # Local Nomic embed (via ONNX runtime)
│   │   │   ├── openai.rs         # OpenAI embedding API client
│   │   │   └── ollama.rs         # Ollama embedding endpoint
│   │   └── Cargo.toml
│   │
│   ├── uc-mcp/                   # MCP server implementation
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── server.rs         # MCP protocol handler
│   │   │   ├── tools.rs          # Tool definitions (uc_store, uc_query, etc.)
│   │   │   └── resources.rs      # Resource definitions (uc://context/auto, etc.)
│   │   └── Cargo.toml
│   │
│   ├── uc-proxy/                 # OpenAI-compatible API proxy
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── middleware.rs     # Context injection middleware
│   │   │   ├── routes.rs         # /v1/chat/completions proxy
│   │   │   └── config.rs
│   │   └── Cargo.toml
│   │
│   └── uc-cli/                   # CLI tool
│       ├── src/
│       │   └── main.rs           # uc store, uc query, uc status, uc rebuild-index
│       └── Cargo.toml
│
├── bindings/
│   └── typescript/               # TypeScript/Node.js bindings via napi-rs
│       ├── src/
│       │   └── lib.rs            # napi-rs bridge
│       ├── index.ts              # TypeScript type definitions
│       └── package.json          # @unlimited-context/sdk
│
├── config/
│   └── default.toml              # Default configuration
│
└── docs/
    ├── architecture.md
    ├── api-reference.md
    └── deployment.md
```

---

## 12. Implementation Phases

### Phase 1: Core Engine (Weeks 1-4)

**Goal:** Store and retrieve text chunks using Arweave + LanceDB.

- [ ] `uc-arweave`: Arweave client with transaction creation, signing, and submission via ar.io Turbo
- [ ] `uc-arweave`: GraphQL query builder for tag-based transaction discovery
- [ ] `uc-core/chunker`: Basic text chunking (fixed-size with overlap, conversation-turn-aware)
- [ ] `uc-core/tagger`: Tag generation from chunk metadata
- [ ] `uc-core/batcher`: Local buffer with time/count flush triggers
- [ ] `uc-core/writer`: Batch serialization and Arweave submission
- [ ] `uc-core/index`: LanceDB schema creation, write, and vector search
- [ ] `uc-embeddings`: Nomic local embedding via ONNX runtime
- [ ] `uc-cli`: Basic `uc store <text>` and `uc query <text>` commands
- [ ] End-to-end test: store → Arweave → index → retrieve

### Phase 2: Retrieval Intelligence (Weeks 5-7)

**Goal:** Multi-strategy retrieval with reranking and context assembly.

- [ ] `uc-core/retriever`: Multi-strategy retrieval (vector + recency + metadata)
- [ ] `uc-core/reranker`: Cross-encoder reranking (optional, via local model or API)
- [ ] `uc-core/assembler`: Context formatting with XML structure, token budgeting
- [ ] `uc-core/assembler`: Basic context compression (truncation, summarization hooks)
- [ ] Query analyzer: temporal reference detection, explicit reference detection
- [ ] Index rebuild from Arweave (cold start path)
- [ ] Index snapshot storage and restore

### Phase 3: Integration Interfaces (Weeks 8-10)

**Goal:** Expose engine via MCP, API proxy, and SDK.

- [ ] `uc-mcp`: MCP server with tools and resources
- [ ] `uc-proxy`: OpenAI-compatible proxy with context injection
- [ ] `bindings/typescript`: napi-rs TypeScript bindings
- [ ] `@unlimited-context/sdk` npm package
- [ ] Configuration system (TOML file, env vars, CLI flags)
- [ ] Integration tests with Claude Code (MCP), OpenClaw (SDK)

### Phase 4: Encryption & Sharing (Weeks 11-13)

**Goal:** End-to-end encryption, logical deletion, selective sharing.

- [ ] `uc-core/crypto`: AES-256-GCM batch encryption
- [ ] Master key derivation from Arweave wallet
- [ ] Batch key management (encrypted key storage on Arweave)
- [ ] Logical deletion (key destruction + index cleanup + tombstones)
- [ ] Selective sharing (re-encryption for recipient public keys)
- [ ] Share grant discovery via GraphQL

### Phase 5: Hosted Tier & Polish (Weeks 14-18)

**Goal:** Managed SaaS offering, production hardening.

- [ ] Multi-tenant architecture (per-user LanceDB partitions or Qdrant upgrade)
- [ ] User authentication and billing
- [ ] Hosted embedding inference
- [ ] Dedicated ar.io gateway operation
- [ ] Monitoring, alerting, observability
- [ ] Documentation and developer guides
- [ ] Launch

---

## 13. Open Questions

1. **Chunk addressing within batches.** When a retrieval result points to a specific chunk, the consumer needs the Arweave TX ID + the batch index to locate it. Should we also store a content hash per chunk for integrity verification?

2. **Embedding dimensionality trade-off.** Nomic supports Matryoshka embeddings (truncatable from 768 down to 64 dimensions). Lower dimensions = faster search + less disk, but lower recall. Should we default to full 768d or offer configurable dimensionality?

3. **Cross-user knowledge bases.** The current design is single-user. Should the schema support shared knowledge bases (e.g., a team's documentation) from the start, or add it later?

4. **Arweave wallet management.** Should the product manage Arweave wallets for users (simpler UX, custody risk) or require users to bring their own wallet (more complex, self-sovereign)?

5. **Gateway strategy for MVP.** Self-hosting an ar.io gateway requires a 10,000 ARIO token stake. For the MVP, should we rely on public gateways and defer self-hosting to the hosted tier?

6. **Naming.** Is "Unlimited Context" the product name, or a working title?

---

## 14. Risks & Mitigations

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Arweave L1 confirmation latency (~2 min) causes UX friction | Medium | High | Local buffer with instant local confirmation; async Arweave settlement |
| ar.io Turbo has no Rust SDK (TypeScript only) | Medium | Confirmed | Wrap Turbo HTTP API directly in Rust; Turbo is just an HTTP endpoint |
| LanceDB hits scale limits beyond 200M vectors | Low | Low | Migration path to Qdrant for hosted tier; LanceDB team actively scaling |
| AR token price spike makes storage expensive | Medium | Medium | Pre-purchase storage credits; offer fiat pricing abstraction via Turbo x402 |
| Embedding model quality degrades retrieval | Medium | Low | Model-agnostic design; easy to swap models and reindex |
| GDPR compliance challenges with immutable storage | High | Medium | Encrypt-by-default; logical deletion via key destruction; tombstone transactions |
| Cold start too slow for large context histories | Medium | Medium | Index snapshots on Arweave; incremental rebuild from last snapshot |

---

*End of Technical Specification*
