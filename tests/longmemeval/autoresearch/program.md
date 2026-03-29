# Memoryport LongMemEval Autoresearch Program

## Objective

Maximize LongMemEval answer accuracy on `longmemeval_s` (the standard difficulty split with ~115K token haystacks per question) while keeping query latency at 500M token scale under 500ms.

## Optimization Target

**Primary metric:** Answer accuracy (%) on a 100-question balanced sample from `longmemeval_s`
**Secondary metric:** Session recall (%) — must not regress below baseline
**Constraint:** Query latency p50 at 500M tokens must stay under 500ms (test with scale benchmark if architectural changes are made)

## What You Can Modify

You are an AI research agent. You may modify ANY Rust source code in the `crates/uc-core/src/` directory AND the experiment config. The key files are:

### Config Parameters (fast to test — no recompile needed if exposed via config)
- `similarity_top_k` (default: 50) — candidate pool size
- `min_relevance_score` (default: 0.3) — quality gate threshold
- `recency_window` (default: 20) — recent chunks to include
- `rerank` (default: false) — enable heuristic reranking
- `query_expansion` (default: false) — LLM-based query reformulation
- `hyde` (default: false) — Hypothetical Document Embeddings
- `max_context_tokens` (default: 50,000) — token budget for assembly

### Retriever Constants (require `cargo build`)
- RRF k constant (default: 60.0) in `retriever.rs`
- Session diversity cap (default: 5 per session) in `retriever.rs`
- Expanded query top_k divisor (default: /3) in `retriever.rs`
- Explicit session top_k (default: 20) in `retriever.rs`

### Reranker Parameters (require `cargo build`)
- `recency_half_life_ms` (default: 86,400,000 = 1 day)
- `session_affinity_boost` (default: 1.2)
- `diversity_lambda` (default: 0.7) — MMR tradeoff
- Recency weight split (default: 0.7 base + 0.3 recency)

### Gate Parameters (require `cargo build`)
- Gate 2 `retrieve_bias` (default: 0.05) in `gate.rs`
- Gate 1 patterns in `analyzer.rs`
- Gate 2 exemplars (20 retrieve + 20 skip) in `gate.rs`

### Chunker Parameters (require `cargo build` + re-ingest)
- `target_size` (default: 1,500 chars)
- `overlap` (default: 200 chars)

### Enhancer Parameters (require `cargo build`)
- Expansion count (default: 5)
- HyDE prompt text
- Query expansion prompt text

### Assembler Parameters (require `cargo build`)
- Context format / XML structure
- Dedup fingerprint length (default: 100 chars)
- Token budget allocation strategy

## Experiment Rules

1. **One change at a time.** Each experiment should test a single hypothesis. If you want to test a combination, first test each component individually.

2. **Always build before running.** If you modified Rust code, run `cargo build -p uc-server` and verify it succeeds before running the benchmark.

3. **Never modify `prepare.py`** — it is the immutable benchmark harness.

4. **Never modify `program.md`** — these are your instructions.

5. **Log every experiment** in `results.tsv` with: commit hash, overall accuracy, per-type accuracy breakdown, session recall, latency p50, description of change.

6. **Revert failed experiments.** If accuracy drops, revert the change before trying the next experiment. Use `git checkout -- <file>` to revert.

7. **Build time budget:** Each experiment cycle (build + ingest + evaluate) should complete within 30 minutes. If an experiment will take longer, skip it and note why.

8. **The `/v1/retrieve` endpoint bypasses gating.** The benchmark calls `/v1/retrieve` directly, so Gate 1 and Gate 2 do NOT affect benchmark results. Focus on retrieval algorithm quality, not gating.

9. **Temporal reasoning is the weakest category.** Prioritize experiments that improve temporal reasoning without hurting other categories.

10. **The `reference_time` parameter is available.** The benchmark passes the question date as `reference_time` for temporal queries. Make sure temporal filtering logic uses this correctly.

## Research Directions (suggested priority order)

### Phase 1: Low-hanging fruit (config-only)
- [ ] Enable reranking and measure impact
- [ ] Enable query expansion (with OpenAI) and measure impact
- [ ] Enable HyDE and measure impact
- [ ] Tune `similarity_top_k` (try 30, 75, 100)
- [ ] Tune `min_relevance_score` (try 0.1, 0.2, 0.5)

### Phase 2: Retrieval algorithm improvements
- [ ] Improve temporal range detection for LongMemEval-style questions
- [ ] Add temporal boosting: boost results closer to `reference_time` in scoring
- [ ] Improve RRF parameters (try k=20, k=40, k=80)
- [ ] Increase session diversity cap (try 3, 8, 10)
- [ ] Improve fact-based retrieval for knowledge-update questions

### Phase 3: Deeper architectural changes
- [ ] Add BM25/keyword hybrid search alongside vector search
- [ ] Implement cross-encoder reranking (using OpenAI or local model)
- [ ] Improve chunk boundaries for multi-turn conversations
- [ ] Add session-level summarization as an additional retrieval key
- [ ] Implement query decomposition for multi-session questions

## Baseline

Run `prepare.py` with default config to establish baseline metrics before making any changes.
