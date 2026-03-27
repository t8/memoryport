#!/usr/bin/env python3
"""
Query-only benchmark — measures retrieval latency on existing data.
Does NOT store any new chunks. Just fires queries and measures timing.

Usage:
    python3 tests/scale/query_bench.py [--server http://127.0.0.1:8090] [--runs 5]
"""

import argparse
import json
import math
import sys
import time

import requests

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
    "How does the proxy inject context into messages?",
    "What embedding model is being used?",
    "How does the three-gate retrieval system work?",
    "What Arweave transactions have been uploaded?",
    "How is encryption implemented for stored data?",
    "What is the session management strategy?",
    "How does the knowledge graph work?",
    "What are the chunk size defaults?",
    "How does the MCP server expose memory tools?",
    "What benchmarks have been run on the system?",
]


def percentile(data, p):
    if not data:
        return 0
    s = sorted(data)
    idx = (p / 100) * (len(s) - 1)
    lo, hi = int(math.floor(idx)), int(math.ceil(idx))
    if lo == hi:
        return s[lo]
    frac = idx - lo
    return s[lo] * (1 - frac) + s[hi] * frac


def main():
    parser = argparse.ArgumentParser(description="Query-only benchmark")
    parser.add_argument("--server", default="http://127.0.0.1:8090")
    parser.add_argument("--runs", type=int, default=5, help="Runs per query")
    parser.add_argument("--output", default="tests/scale/results_query.json")
    args = parser.parse_args()

    # Check server
    try:
        r = requests.get(f"{args.server}/v1/status", timeout=10)
        status = r.json()
        chunks = status.get("indexed_chunks", "?")
        model = status.get("embedding_model", "?")
    except Exception as e:
        print(f"Error: Server not ready at {args.server}: {e}", file=sys.stderr)
        sys.exit(1)

    tokens = int(chunks) * 375 if isinstance(chunks, int) else 0

    print(f"Query Benchmark")
    print(f"Server: {args.server}")
    print(f"Chunks: {chunks:,} (~{tokens:,} tokens)")
    print(f"Model: {model}")
    print(f"Queries: {len(QUERIES)} x {args.runs} runs = {len(QUERIES) * args.runs} total")
    print()

    # Warmup
    print("Warming up (2 queries)...")
    for q in QUERIES[:2]:
        requests.post(f"{args.server}/v1/retrieve", json={"query": q, "top_k": 10}, timeout=30)
    print()

    # Benchmark
    latencies = []
    for i, q in enumerate(QUERIES):
        query_latencies = []
        for run in range(args.runs):
            start = time.time()
            try:
                r = requests.post(
                    f"{args.server}/v1/retrieve",
                    json={"query": q, "top_k": 10},
                    timeout=30,
                )
                r.raise_for_status()
                ms = (time.time() - start) * 1000
                query_latencies.append(ms)
            except Exception as e:
                print(f"  Query {i+1} run {run+1} failed: {e}")

        latencies.extend(query_latencies)
        avg = sum(query_latencies) / len(query_latencies) if query_latencies else 0
        print(f"  [{i+1:2d}/{len(QUERIES)}] {avg:6.0f}ms avg  {q[:50]}...")

    print()
    print(f"--- Results ({len(latencies)} measurements) ---")
    print(f"  p50:  {percentile(latencies, 50):.0f}ms")
    print(f"  p95:  {percentile(latencies, 95):.0f}ms")
    print(f"  p99:  {percentile(latencies, 99):.0f}ms")
    print(f"  mean: {sum(latencies)/len(latencies):.0f}ms")
    print(f"  min:  {min(latencies):.0f}ms")
    print(f"  max:  {max(latencies):.0f}ms")

    result = {
        "chunks": chunks,
        "tokens": tokens,
        "model": model,
        "queries": len(QUERIES),
        "runs_per_query": args.runs,
        "p50_ms": percentile(latencies, 50),
        "p95_ms": percentile(latencies, 95),
        "p99_ms": percentile(latencies, 99),
        "mean_ms": sum(latencies) / len(latencies),
        "min_ms": min(latencies),
        "max_ms": max(latencies),
    }

    with open(args.output, "w") as f:
        json.dump(result, f, indent=2)
    print(f"\nSaved to {args.output}")


if __name__ == "__main__":
    main()
