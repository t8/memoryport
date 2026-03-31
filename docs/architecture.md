# Memoryport Architecture

This document describes how Memoryport works internally — the data pipelines, storage layers, retrieval strategies, and design tradeoffs. It's written for developers who want to understand, contribute to, or build on top of the system.

## What Memoryport Does

Memoryport gives LLMs persistent memory across conversations, sessions, and tools. It sits as a transparent proxy between your AI tools (Claude Code, Cursor, Ollama, etc.) and the AI providers (Anthropic, OpenAI). Every conversation is captured, indexed, and relevant context is injected into future conversations automatically.

The system has three core capabilities:
1. **Capture** — automatically store conversations from any supported tool
2. **Retrieve** — find relevant past context using hybrid search
3. **Inject** — add retrieved context to the LLM's prompt transparently

## System Overview

```
User's AI tool (Claude Code, Cursor, etc.)
        │
        ▼
┌─────────────────────┐
│    API Proxy         │  Intercepts on localhost:9191
│    (uc-proxy)        │  Anthropic + OpenAI + Ollama APIs
└────────┬────────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
┌────────┐  ┌──────────────┐
│ Forward │  │ Core Engine   │
│ to LLM  │  │ (uc-core)     │
│ provider│  │               │
└────────┘  │ ┌───────────┐ │
            │ │ Chunks DB  │ │  LanceDB table: raw conversation text
            │ └───────────┘ │
            │ ┌───────────┐ │
            │ │ Facts DB   │ │  LanceDB table: extracted atomic facts
            │ └───────────┘ │
            │ ┌───────────┐ │
            │ │ Arweave    │ │  Permanent encrypted backup (optional)
            │ └───────────┘ │
            └──────────────┘
```

## Store Pipeline

When text is stored (via proxy capture, MCP auto-store, CLI, or API):

```
Text arrives
  │
  ├─ 1. Chunk (chunker.rs)
  │     Split into ~375-token chunks at sentence boundaries
  │     Each chunk gets: UUID, session_id, role, timestamp
  │
  ├─ 2. Buffer (batcher.rs)
  │     Accumulate up to 50 chunks, flush on count or 60s timer
  │
  ├─ 3. Embed (uc-embeddings)
  │     Batch embed all chunk texts via configured provider (OpenAI or Ollama)
  │
  ├─ 4. Upload to Arweave (writer.rs, optional)
  │     Serialize → optionally encrypt (AES-256-GCM) → upload via Turbo
  │
  ├─ 5. Index chunks (index.rs)
  │     Insert into LanceDB "chunks" table with vectors
  │
  ├─ 6. Extract facts (facts.rs)
  │     NLP regex patterns extract subject-predicate-object triples
  │     Categories: preferences, personal info, projects, temporal, knowledge updates
  │     Each fact gets dual timestamps: document_date + event_date
  │
  ├─ 7. Embed facts
  │     Batch embed fact content (reuses same embedding provider)
  │
  ├─ 8. Detect contradictions (contradiction.rs)
  │     For each new fact, search existing facts with same subject + conflicting predicate
  │     If found: mark old fact valid=false, set superseded_by
  │     Example: "lives_in NYC" superseded by "moved_to London"
  │
  └─ 9. Index facts (index.rs)
        Insert into LanceDB "facts" table with vectors
```

### What Gets Extracted as Facts

The NLP extraction (facts.rs) uses regex patterns to identify structured information. It does NOT use an LLM — this keeps ingestion fast and free of API costs.

**Patterns matched:**
- Preferences: "I prefer X", "my favorite X is Y", "I use X", "I like X"
- Personal info: "my name is X", "I live in X", "I work at X", "I'm a X"
- Projects: "working on X", "building X", "the project is called X"
- Temporal: "I started X on Y", "I moved to X", "I switched to X"
- Knowledge updates: "actually X", "correction: X" (flags contradictions)

**Limitations of NLP extraction:**
- Misses implicit facts ("The meeting went well" doesn't extract the meeting as a fact)
- Can produce false positives on complex sentences
- Doesn't handle coreference ("He moved to London" — who is "he"?)
- Extraction quality is lower than LLM-based extraction (which systems like Supermemory and Graphiti use)

The tradeoff is intentional: zero LLM cost at ingestion, runs locally, works offline. An optional LLM extraction mode can be enabled for higher quality at the cost of API calls.

### Contradiction Resolution

When fact extraction finds a new fact like "I moved to London", it searches the facts table for existing facts with:
- Same subject (case-insensitive, with self-reference normalization: "I" = "user" = "me")
- Conflicting predicate (predicate groups: lives_in/moved_to/based_in are all the same group)
- Different object

If found, the old fact is marked `valid=false` with `superseded_by` pointing to the new fact's ID. The old fact is never deleted — it remains in the index for temporal history queries ("where did I live before?").

**What this gets right:** Explicit preference/location/job changes with clear language.

**What this misses:** Implicit updates, complex multi-fact contradictions, facts where the subject isn't a self-reference.

## Retrieve Pipeline

When a query arrives (via proxy context injection, MCP query, CLI, or API):

```
Query text
  │
  ├─ 1. Analyze (analyzer.rs)
  │     Extract signals: temporal range, explicit session refs, recency
  │     Gate 1 decision: Skip (greeting), Force (memory ref), or Undecided
  │     Uses reference_time parameter if provided (defaults to now)
  │
  ├─ 2. Enhance (enhancer.rs, optional)
  │     Query expansion: generate alternative phrasings
  │     HyDE: embed a hypothetical answer instead of the raw query
  │     Requires LLM — disabled by default
  │
  ├─ 3. Embed query
  │     Convert query text to vector via configured embedding provider
  │
  ├─ 4. Gate 2: Embedding routing (gate.rs)
  │     Compare query vector against "retrieve" vs "skip" centroids
  │     Only active when index has 100+ chunks
  │
  ├─ 5. Hybrid search (retriever.rs, parallel)
  │     ├─ Vector search on chunks table (primary)
  │     ├─ Vector search on facts table
  │     ├─ Expanded query searches (if enhanced)
  │     ├─ Recency window (recent chunks from active session)
  │     └─ Explicit session lookup (if session reference detected)
  │
  ├─ 6. RRF fusion (retriever.rs)
  │     Reciprocal Rank Fusion merges chunk and fact results
  │     score(d) = sum(1 / (k + rank + 1)) across all result sets
  │     k = 60.0 (standard constant)
  │     Facts that match become synthetic Knowledge-type results
  │
  ├─ 7. Session diversity
  │     Cap at 5 chunks per session to prevent one session dominating
  │
  ├─ 8. Gate 3: Quality threshold
  │     Drop all results if best score < min_relevance_score
  │
  ├─ 9. Rerank (reranker.rs)
  │     Recency boost (exponential decay, 1-day half-life)
  │     Session affinity boost (1.2x for active session)
  │     MMR diversity (70% relevance, 30% diversity via Jaccard)
  │
  └─ 10. Assemble context (assembler.rs)
        Deduplicate near-identical content
        Token budget greedy fill
        XML formatting with session dates, turn times, roles
        Current date injected for temporal reasoning
```

### Two Retrieval Modes

**`retrieve` (original):** Vector search on chunks only. Used by the `search` engine method for direct user searches (search bars, CLI).

**`retrieve_hybrid` (default for query/retrieve):** Parallel chunk + fact search with RRF fusion. Used by the `query` and `retrieve` engine methods, which are what the proxy and MCP server call.

### Why RRF?

Reciprocal Rank Fusion merges results from different retrieval strategies without requiring a trained model. Each strategy ranks results independently, and RRF assigns scores based on rank position rather than raw similarity scores (which aren't comparable across strategies).

In practice: a chunk that ranks #2 in vector search and #5 in fact search gets a higher RRF score than a chunk that ranks #1 in vector search but doesn't appear in fact search at all. This rewards results that are relevant across multiple signals.

**Known limitation:** With noisy fact extraction, low-quality facts can displace relevant chunks. The current mitigation is session diversity capping. Future work: confidence-weighted RRF where low-confidence facts get lower weight.

## Storage Architecture

### LanceDB (Primary Store)

Two tables in a single LanceDB database at `~/.memoryport/index/`:

**`chunks` table:**
| Column | Type | Description |
|--------|------|-------------|
| vector | Float32[N] | Embedding vector (N = configured dimensions) |
| chunk_id | Utf8 | UUID |
| arweave_tx_id | Utf8 | Arweave transaction ID (or local_* for local-only) |
| batch_index | UInt32 | Position within batch |
| user_id | Utf8 | User identifier |
| session_id | Utf8 | Conversation session identifier |
| chunk_type | Utf8 | "conversation", "document", or "knowledge" |
| role | Utf8 (nullable) | "user", "assistant", or "system" |
| timestamp | Int64 | Milliseconds since epoch |
| content | Utf8 | Full chunk text |
| token_count | UInt32 | Estimated token count |
| metadata_json | Utf8 (nullable) | Serialized JSON (source_integration, source_model, etc.) |

Indexes: BTree on user_id, session_id, timestamp. Auto-compaction every 100 inserts.

**`facts` table:**
| Column | Type | Description |
|--------|------|-------------|
| vector | Float32[N] | Embedding of fact content |
| fact_id | Utf8 | UUID |
| content | Utf8 | Atomic fact as a sentence |
| subject | Utf8 | Extracted entity ("user", "Project X") |
| predicate | Utf8 | Relation type ("lives_in", "prefers", "works_at") |
| object | Utf8 | Value ("Austin", "Vim", "Google") |
| source_chunk_id | Utf8 | FK to chunks table |
| session_id | Utf8 | From source chunk |
| user_id | Utf8 | User identifier |
| document_date | Int64 | When the conversation happened (ms epoch) |
| event_date | Int64 (nullable) | When the fact became true (ms epoch) |
| valid | Boolean | true = current, false = superseded |
| superseded_by | Utf8 (nullable) | fact_id of the newer fact |
| confidence | Float32 | 1.0 for explicit patterns, 0.7 for inferred |
| created_at | Int64 | When the fact was indexed |

Indexes: BTree on user_id, subject, predicate, valid.

**Why two tables instead of one?** Chunks contain full conversation context — useful for understanding nuance and providing the LLM with readable text. Facts contain atomic, structured information — useful for precise retrieval and contradiction tracking. Searching both and fusing with RRF gives better results than either alone.

**Why LanceDB?** Embedded (no server process), Rust-native, columnar (fast filtered queries), vector search built-in, supports compaction. The main alternative was SQLite + a vector extension, but LanceDB's native vector support and Arrow-based columnar format provide better query performance at scale.

### Arweave (Permanent Backup)

Optional. When enabled, chunks are serialized as JSON, optionally encrypted with AES-256-GCM, and uploaded to Arweave via the Turbo service. Each batch gets its own encryption key, wrapped with the user's Argon2id-derived master key.

The index can be rebuilt from Arweave — download all transactions, decrypt, re-embed, re-index. This means you can lose your local data and recover from the permanent storage.

### Entity Registry (In-Memory)

The entity registry (`entities.rs`) is an in-memory data structure with JSON file persistence at `~/.memoryport/entities-{user_id}.json`. It tracks deduplicated named entities with aliases.

**Why not in LanceDB?** Entities are low-volume (hundreds, not millions) and don't need vector search. JSON file is simpler, faster, and doesn't add LanceDB table complexity.

### Profile Cache (In-Memory)

The profile cache (`profile.rs`) maintains a pre-computed summary of user facts at `~/.memoryport/profile-{user_id}.json`. Two sections:
- **Static facts:** name, role, location, organization, preferences (overwrite on update)
- **Dynamic facts:** current projects, recent topics, active issues (capped at 20, oldest dropped)

**Why a separate cache?** The profile is designed for 0ms injection — it's always ready to prepend to retrieval results without any search or computation. Updated incrementally as facts are extracted.

**Current status:** The profile cache module exists and is tested, but is not yet wired into the retrieval response assembly. This is planned work.

## Benchmarks

### LongMemEval (ICLR 2025)

LongMemEval tests long-term memory in chat assistants with 500 questions across 6 categories: temporal-reasoning, multi-session, knowledge-update, single-session-user, single-session-assistant, single-session-preference.

**Our results (LongMemEval-S standard variant, GPT-4o reader):**

| Category | Accuracy | Session Recall | n |
|----------|----------|----------------|---|
| single-session-assistant | 91.1% | 87% | 56 |
| single-session-user | 60.0% | 56% | 70 |
| knowledge-update | 53.3% | 72% | 78 |
| single-session-preference | 36.7% | 53% | 30 |
| temporal-reasoning | 27.1% | 36% | 133 |
| multi-session | 27.1% | 47% | 133 |
| **Overall** | **43.5%** | **61.1%** | **500** |

**For comparison:**
- Supermemory: 85.2% (Gemini-3-pro) — cloud-hosted, LLM-based fact extraction
- Zep/Graphiti: 71.2% (GPT-4o) — Neo4j knowledge graph, 6-10 LLM calls per episode
- Observational Memory (SOTA): 94.87% (GPT-5-mini) — two-agent compression, no vector DB

**Why we score lower:** Our fact extraction is NLP-based (regex patterns), not LLM-based. This means lower precision in extracting facts, which limits our performance on temporal-reasoning (requires understanding when events happened) and multi-session (requires linking entities across conversations). Our retrieval quality (session recall 61.1%) is the primary bottleneck — when we DO find the right context, the LLM answers correctly at high rates (91.1% for single-session-assistant where recall is 87%).

**What we optimize for instead:** Zero LLM cost at ingestion, runs fully local, 294ms query latency at 500M tokens, encrypted permanent storage. The benchmark tradeoff is deliberate — we prioritize privacy, speed, and cost over maximum accuracy.

### Scale Performance

| Context Space | Chunks | p50 Latency |
|--------------|--------|-------------|
| 100K tokens | 266 | 1ms |
| 1M tokens | 2,666 | 3ms |
| 10M tokens | 26,666 | 9ms |
| 100M tokens | 266,666 | 61ms |
| 500M tokens | 1,333,333 | 294ms |

Brute-force vector search with 100% recall. No approximate indexing (IVF-PQ was tested and was slower at this scale). Compacted LanceDB, local embeddings via Ollama.

## Module Reference

### uc-core (Core Engine)

| Module | Purpose | Tests |
|--------|---------|-------|
| chunker.rs | Split text into chunks at sentence boundaries | 5 |
| facts.rs | NLP-based fact extraction (regex patterns) | 68 |
| entities.rs | Entity registry with Jaro-Winkler dedup | 18 |
| profile.rs | User profile cache (static + dynamic facts) | 15 |
| contradiction.rs | Fact contradiction detection and resolution | 9 |
| index.rs | LanceDB storage (chunks + facts tables) | 0 (integration) |
| retriever.rs | Hybrid retrieval with RRF fusion | 4 |
| reranker.rs | Heuristic reranking with MMR diversity | 3 |
| assembler.rs | Context formatting (XML with dates) | 3+ |
| analyzer.rs | Query signal extraction and gating | 30+ |
| gate.rs | Embedding-based retrieval routing | 0 (integration) |
| enhancer.rs | Query expansion and HyDE | 0 (integration) |
| batcher.rs | Chunk buffering and flush management | 2 |
| writer.rs | Arweave upload with optional encryption | 0 (integration) |
| crypto.rs | AES-256-GCM encryption, Argon2id key derivation | 3 |
| keystore.rs | SQLite-backed encryption key storage | 4 |
| tagger.rs | Arweave ANS-104 tag generation | 2 |

### Other Crates

| Crate | Purpose |
|-------|---------|
| uc-arweave | Arweave client (wallet, ANS-104, Turbo uploads, GraphQL) |
| uc-embeddings | Embedding + LLM providers (OpenAI, Ollama) |
| uc-cli | CLI binary with setup wizard |
| uc-mcp | MCP server (7 tools, 2 resources) |
| uc-proxy | Multi-protocol API proxy |
| uc-server | Multi-tenant HTTP API server + dashboard |
| uc-tauri | Tauri desktop app wrapper |

## Design Tradeoffs

### NLP vs LLM Fact Extraction

We chose NLP (regex patterns) over LLM-based extraction. This means:
- **Pro:** Zero API cost, runs offline, fast ingestion, no rate limits
- **Con:** Lower extraction quality, misses implicit facts, no coreference resolution
- **Impact:** ~20-30% lower accuracy on LongMemEval vs LLM-based systems
- **Mitigation:** Optional LLM mode planned; contradiction resolution catches some errors

### Single LanceDB vs Graph Database

We use LanceDB for both chunks and facts instead of adding a graph database (Neo4j, SurrealDB). This means:
- **Pro:** No additional dependency, simpler deployment, works embedded
- **Con:** No native graph traversal, multi-hop queries require application-level joins
- **Impact:** Multi-session queries that require following entity relationships across sessions are weaker
- **Mitigation:** RRF fusion + session diversity partially compensate; multi-turn retrieval lets the LLM do iterative exploration

### Hybrid Retrieval vs Pure Vector Search

We search chunks and facts in parallel and fuse with RRF. This means:
- **Pro:** Higher precision on questions that match extracted facts
- **Con:** Noisy fact extraction can displace relevant chunks
- **Impact:** +4pp on LongMemEval oracle variant, but marginal on standard variant with large haystacks
- **Mitigation:** Session diversity cap (5 chunks/session), planned confidence-weighted RRF

### Transparent Proxy vs Explicit API

We capture conversations via an API proxy rather than requiring explicit store calls. This means:
- **Pro:** Zero integration effort, works with any tool that speaks Anthropic/OpenAI/Ollama API
- **Con:** Can capture noise (system prompts, meta-requests, tool artifacts)
- **Impact:** Requires careful content sanitization to prevent storing internal prompts
- **Mitigation:** Extensive sanitization rules in the proxy for Claude Code, Open WebUI, and Ollama meta-requests

## Future Work

- **LLM-based fact extraction (optional):** Use the user's configured model for higher-quality extraction when they opt in
- **Profile injection in retrieval:** Wire the profile cache into context assembly
- **Confidence-weighted RRF:** Weight fact results by extraction confidence to reduce noise
- **Entity-based cross-session retrieval:** Use the entity registry to find related sessions when a query mentions a known entity
- **Time-aware query expansion:** Resolve relative temporal references ("last month") into absolute date ranges using the reference_time
- **Full-text search (BM25):** LanceDB supports Tantivy-based FTS — adding keyword search as a third retrieval signal alongside vector search on chunks and facts
