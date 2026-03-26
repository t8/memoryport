#!/usr/bin/env python3
"""
Stress test: Benchmark retrieval quality and latency at scale.

Usage:
    python3 tests/stress/benchmark.py [--server http://127.0.0.1:8090]
"""

import argparse
import json
import time
import statistics
import requests

SERVER = "http://127.0.0.1:8090"

# Test queries with expected topic matches
TEST_QUERIES = [
    # Auth queries — should find auth-related chunks
    {"query": "How do we implement JWT authentication?", "expected_topic": "auth"},
    {"query": "The OAuth callback is failing", "expected_topic": "auth"},
    {"query": "API key hashing strategy", "expected_topic": "auth"},
    {"query": "rate limiting for login", "expected_topic": "auth"},
    {"query": "session cookies vs tokens", "expected_topic": "auth"},
    # Database queries
    {"query": "query optimization and indexing", "expected_topic": "database"},
    {"query": "connection pool exhaustion", "expected_topic": "database"},
    {"query": "N+1 query problem", "expected_topic": "database"},
    {"query": "database migration rollback", "expected_topic": "database"},
    {"query": "sharding strategy for large tables", "expected_topic": "database"},
    # Frontend queries
    {"query": "React re-rendering performance", "expected_topic": "frontend"},
    {"query": "dark mode implementation", "expected_topic": "frontend"},
    {"query": "bundle size reduction", "expected_topic": "frontend"},
    {"query": "WebSocket connection drops", "expected_topic": "frontend"},
    {"query": "infinite scroll without library", "expected_topic": "frontend"},
    # Deployment queries
    {"query": "Docker build optimization", "expected_topic": "deployment"},
    {"query": "OOM killed in production", "expected_topic": "deployment"},
    {"query": "zero downtime deployment", "expected_topic": "deployment"},
    {"query": "Kubernetes CrashLoopBackOff", "expected_topic": "deployment"},
    {"query": "secrets management in containers", "expected_topic": "deployment"},
    # Debugging queries
    {"query": "race condition in concurrent code", "expected_topic": "debugging"},
    {"query": "memory leak detection", "expected_topic": "debugging"},
    {"query": "timezone offset bug", "expected_topic": "debugging"},
    {"query": "too many open files error", "expected_topic": "debugging"},
    {"query": "test passes alone fails together", "expected_topic": "debugging"},
    # Architecture queries
    {"query": "microservices vs monolith decision", "expected_topic": "architecture"},
    {"query": "eventual consistency handling", "expected_topic": "architecture"},
    {"query": "CQRS pattern for read-heavy workloads", "expected_topic": "architecture"},
    {"query": "API backward compatibility", "expected_topic": "architecture"},
    {"query": "schema evolution strategy", "expected_topic": "architecture"},
    # Gating test — these should NOT trigger retrieval (but we're using direct search)
    {"query": "hello", "expected_topic": None},
    {"query": "thanks", "expected_topic": None},
    {"query": "fix the typo", "expected_topic": None},
    {"query": "run the tests", "expected_topic": None},
    {"query": "git commit", "expected_topic": None},
    # Cross-topic queries
    {"query": "how to handle errors in the authentication database", "expected_topic": "auth"},
    {"query": "deploying the frontend with Docker", "expected_topic": "deployment"},
    {"query": "debugging the React WebSocket connection", "expected_topic": "frontend"},
    {"query": "database schema migration in production", "expected_topic": "database"},
    {"query": "architecture for the event processing pipeline", "expected_topic": "architecture"},
]


def run_benchmark(server: str):
    """Run retrieval benchmark queries and measure quality + latency."""
    latencies = []
    relevant_hits = 0
    total_queries = 0
    results_per_query = []

    print(f"Running {len(TEST_QUERIES)} benchmark queries...\n")

    for tq in TEST_QUERIES:
        query = tq["query"]
        expected = tq["expected_topic"]

        start = time.time()
        try:
            resp = requests.post(
                f"{server}/v1/retrieve",
                json={"query": query, "top_k": 10},
                timeout=30,
            )
            elapsed_ms = (time.time() - start) * 1000
            latencies.append(elapsed_ms)

            if resp.status_code != 200:
                print(f"  ERROR {resp.status_code}: {query[:50]}")
                continue

            results = resp.json().get("results", [])
            total_queries += 1
            results_per_query.append(len(results))

            # Check relevance: does any result's session_id contain the expected topic?
            if expected:
                hit = any(expected in r.get("session_id", "") for r in results[:10])
                if hit:
                    relevant_hits += 1
                else:
                    print(f"  MISS: '{query[:50]}' expected={expected}, got sessions={[r['session_id'][:20] for r in results[:3]]}")
            else:
                # Greeting/command — we expect results (direct search bypasses gating)
                pass

        except Exception as e:
            print(f"  EXCEPTION: {e}")

    # Report
    print(f"\n{'='*60}")
    print(f"BENCHMARK RESULTS ({total_queries} queries)")
    print(f"{'='*60}")

    if latencies:
        print(f"\nLatency:")
        print(f"  p50:  {statistics.median(latencies):.1f}ms")
        print(f"  p95:  {sorted(latencies)[int(len(latencies)*0.95)]:.1f}ms")
        print(f"  p99:  {sorted(latencies)[int(len(latencies)*0.99)]:.1f}ms")
        print(f"  max:  {max(latencies):.1f}ms")
        print(f"  mean: {statistics.mean(latencies):.1f}ms")

    topic_queries = sum(1 for tq in TEST_QUERIES if tq["expected_topic"])
    if topic_queries > 0:
        print(f"\nRetrieval Quality:")
        print(f"  Relevant hits: {relevant_hits}/{topic_queries} ({relevant_hits/topic_queries*100:.0f}%)")

    if results_per_query:
        print(f"\nResults per query:")
        print(f"  mean: {statistics.mean(results_per_query):.1f}")
        print(f"  min:  {min(results_per_query)}")
        print(f"  max:  {max(results_per_query)}")

    # Check status
    try:
        status = requests.get(f"{server}/v1/status").json()
        print(f"\nIndex: {status['indexed_chunks']} chunks")
    except:
        pass


def main():
    parser = argparse.ArgumentParser(description="Retrieval quality benchmark")
    parser.add_argument("--server", type=str, default=SERVER, help="Server URL")
    args = parser.parse_args()
    run_benchmark(args.server)


if __name__ == "__main__":
    main()
