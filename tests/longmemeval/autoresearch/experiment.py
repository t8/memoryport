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
    # ── BEST CONFIG (Exp 8): 61% accuracy, 64.75% session recall, 337ms p50 ──
    "retrieval": {
        "similarity_top_k": 150,     # 3x default (was 50)
        "min_relevance_score": 0.3,
        "recency_window": 20,
        "max_context_tokens": 50000,
        "rerank": False,
        "query_expansion": False,
        "hyde": False,
        "gating_enabled": True,
    },

    "context_chunks": 40,            # 2x default (was 20)
    "prompt_style": "default",
    "answer_model": "gpt-4o",        # gpt-4o >> gpt-4o-mini for reasoning
    "judge_model": "gpt-4o-mini",

    "description": "BEST: Exp 8 — top_k=150, temporal fallback, 40 context chunks, gpt-4o",
}
