#!/usr/bin/env python3
"""
Clean benchmark: measures query latency at multiple context space sizes
by creating temporary Lance datasets of each size from the main index.
Does NOT modify the original database.

Usage:
    PYTHONUNBUFFERED=1 python3 tests/scale/clean_bench.py
"""

import json
import math
import os
import shutil
import time
from datetime import datetime

import lance
import requests

TOKENS_PER_CHUNK = 375
INDEX_PATH = os.path.expanduser("~/.memoryport/index/chunks.lance")
TEMP_DIR = os.path.expanduser("~/.memoryport/bench_temp")
OLLAMA_URL = "http://localhost:11434/api/embed"
OUTPUT = "tests/scale/results_clean.json"

# Checkpoints in tokens
CHECKPOINTS = [
    100_000,       # 267 chunks
    1_000_000,     # 2,667 chunks
    5_000_000,     # 13,333 chunks
    10_000_000,    # 26,667 chunks
    50_000_000,    # 133,333 chunks
    100_000_000,   # 266,667 chunks
    250_000_000,   # 666,667 chunks
    500_000_000,   # 1,333,333 chunks
]

QUERIES = [
    "How do I set up authentication with JWT tokens?",
    "What database migrations have been applied recently?",
    "Explain the caching strategy for API responses",
    "How does the deployment pipeline work?",
    "What error handling patterns are used in the codebase?",
    "How is rate limiting implemented?",
    "What testing frameworks are configured?",
    "How does the WebSocket connection management work?",
    "What environment variables are required for production?",
    "How is logging structured across services?",
]


def log(msg):
    ts = datetime.now().strftime("%H:%M:%S")
    print(f"[{ts}] {msg}", flush=True)


def get_query_vectors():
    """Pre-compute all query embeddings once."""
    log("Computing query embeddings via Ollama...")
    texts = QUERIES
    r = requests.post(OLLAMA_URL, json={"model": "nomic-embed-text", "input": texts}, timeout=120)
    r.raise_for_status()
    return r.json()["embeddings"]


def create_subset(ds, num_rows, dest_path):
    """Create a temporary Lance dataset with num_rows from the source."""
    if os.path.exists(dest_path):
        shutil.rmtree(dest_path)

    # Take first N rows
    scanner = ds.scanner(columns=None, limit=num_rows)
    table = scanner.to_table()

    lance.write_dataset(table, dest_path, mode="create")
    temp_ds = lance.dataset(dest_path)

    # Compact
    temp_ds.optimize.compact_files(target_rows_per_fragment=num_rows + 1)
    temp_ds.cleanup_old_versions(older_than=__import__("datetime").timedelta(seconds=1))

    return temp_ds


def benchmark_dataset(ds, query_vectors, runs_per_query=3):
    """Run queries against a Lance dataset and return latency stats."""
    latencies = []

    for vec in query_vectors:
        for _ in range(runs_per_query):
            start = time.time()
            results = ds.to_table(
                nearest={"column": "vector", "q": vec, "k": 10},
                columns=["chunk_id", "session_id", "chunk_type", "role",
                         "timestamp", "content", "arweave_tx_id"],
            )
            ms = (time.time() - start) * 1000
            latencies.append(ms)

    latencies.sort()
    n = len(latencies)
    return {
        "p50": latencies[n // 2],
        "p95": latencies[int(n * 0.95)],
        "p99": latencies[int(n * 0.99)],
        "mean": sum(latencies) / n,
        "min": latencies[0],
        "max": latencies[-1],
    }


def main():
    log("=" * 60)
    log("Clean Scale Benchmark")
    log("=" * 60)

    # Open main dataset
    ds = lance.dataset(INDEX_PATH)
    total_rows = ds.count_rows()
    total_tokens = total_rows * TOKENS_PER_CHUNK
    log(f"Main dataset: {total_rows:,} rows ({total_tokens / 1e6:.0f}M tokens)")

    # Pre-compute query vectors (one Ollama call)
    query_vectors = get_query_vectors()
    log(f"Query vectors: {len(query_vectors)} queries embedded")

    # Add the actual total as final checkpoint
    checkpoints = [c for c in CHECKPOINTS if c // TOKENS_PER_CHUNK <= total_rows]
    checkpoints.append(total_rows * TOKENS_PER_CHUNK)

    results = []
    os.makedirs(TEMP_DIR, exist_ok=True)

    for checkpoint in checkpoints:
        target_chunks = checkpoint // TOKENS_PER_CHUNK
        actual_chunks = min(target_chunks, total_rows)
        actual_tokens = actual_chunks * TOKENS_PER_CHUNK

        log(f"\n--- {actual_tokens / 1e6:.0f}M tokens ({actual_chunks:,} chunks) ---")

        if actual_chunks == total_rows:
            # Use the main dataset directly (no copy needed)
            log("  Using main dataset directly")
            bench_ds = ds
        else:
            # Create temporary subset
            temp_path = os.path.join(TEMP_DIR, f"bench_{actual_chunks}")
            log(f"  Creating subset ({actual_chunks:,} rows)...")
            start = time.time()
            bench_ds = create_subset(ds, actual_chunks, temp_path)
            log(f"  Subset created in {time.time() - start:.1f}s")

        # Benchmark
        log(f"  Querying (10 queries x 3 runs)...")
        stats = benchmark_dataset(bench_ds, query_vectors, runs_per_query=3)

        result = {
            "tokens": actual_tokens,
            "chunks": actual_chunks,
            "p50_ms": round(stats["p50"], 1),
            "p95_ms": round(stats["p95"], 1),
            "p99_ms": round(stats["p99"], 1),
            "mean_ms": round(stats["mean"], 1),
        }
        results.append(result)

        log(f"  p50={stats['p50']:.0f}ms  p95={stats['p95']:.0f}ms  mean={stats['mean']:.0f}ms")

    # Cleanup temp dir
    if os.path.exists(TEMP_DIR):
        shutil.rmtree(TEMP_DIR)
        log("\nTemp files cleaned up")

    # Save results
    with open(OUTPUT, "w") as f:
        json.dump(results, f, indent=2)
    log(f"Results saved to {OUTPUT}")

    # Print table
    log("\n" + "=" * 60)
    log("| Context Space | Chunks | p50 (ms) | p95 (ms) | Mean (ms) |")
    log("|---|---|---|---|---|")
    for r in results:
        tokens = r["tokens"]
        label = f"{tokens/1e6:.0f}M" if tokens >= 1e6 else f"{tokens/1e3:.0f}K"
        log(f"| {label} | {r['chunks']:,} | {r['p50_ms']:.0f} | {r['p95_ms']:.0f} | {r['mean_ms']:.0f} |")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        log("\nInterrupted")
        if os.path.exists(TEMP_DIR):
            shutil.rmtree(TEMP_DIR)
    except Exception as e:
        log(f"Error: {e}")
        import traceback
        traceback.print_exc()
        if os.path.exists(TEMP_DIR):
            shutil.rmtree(TEMP_DIR)
