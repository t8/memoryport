"""
Experiment configuration for autoresearch.

THIS FILE IS THE AGENT'S SANDBOX. Modify CONFIG to test hypotheses.
Each experiment should change one thing at a time.

After modifying, run:
    python3 tests/longmemeval/autoresearch/prepare.py

If you modified Rust code, omit --skip-build.
If the index is already ingested for the same dataset, add --skip-ingest.
"""

# ── Experiment Config ───────────────────────────────────────────────────────
# This is the BASELINE configuration. The agent modifies this dict.

CONFIG = {
    # Base: Exp 8 config (61% accuracy)
    "retrieval": {
        "similarity_top_k": 150,
        "min_relevance_score": 0.3,
        "recency_window": 20,
        "max_context_tokens": 50000,
        "rerank": False,
        "query_expansion": False,
        "hyde": False,
        "gating_enabled": True,
    },

    "context_chunks": 40,
    "prompt_style": "default",
    "answer_model": "gpt-4o",
    "judge_model": "gpt-4o-mini",

    # EXPERIMENT 18: Rust sub-query decomposition in engine.search().
    # Detects multi-entity comparisons ("A or B"), aggregation ("how many"),
    # and temporal ordering queries. Extracts entities and runs parallel
    # sub-queries to cover entities the primary embedding misses.
    # No LLM needed — pure pattern matching.
    "prompt_style": "default",

    "description": "Exp 35: CLEAN BASELINE — temporal fallback + date enrichment only (no BM25, no re-query)",
}
