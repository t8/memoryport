#!/usr/bin/env python3
"""
Context Space Scale Benchmark

Measures query latency as the context space grows from 1K to 500M tokens.
Uses the uc-server HTTP API for store and retrieve operations.

Chunk definition: 1 chunk ≈ 1,500 chars ≈ 375 tokens (4 chars/token)

Usage:
    python3 tests/scale/benchmark.py [--server http://127.0.0.1:8090] [--max-tokens 500000000]
"""

import argparse
import json
import math
import os
import random
import string
import sys
import time
from dataclasses import dataclass
from typing import Optional

import requests

# Chunk constants
CHARS_PER_CHUNK = 1500
TOKENS_PER_CHUNK = 375  # CHARS_PER_CHUNK / 4
OVERLAP_CHARS = 200

# Test queries — diverse topics to test retrieval quality at scale
TEST_QUERIES = [
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

# Topic pools for generating realistic content
TOPICS = [
    "authentication", "database", "caching", "deployment", "testing",
    "monitoring", "api-design", "error-handling", "security", "performance",
    "frontend", "backend", "infrastructure", "ci-cd", "documentation",
    "refactoring", "debugging", "code-review", "architecture", "scaling",
]

TECH_TERMS = [
    "REST API", "GraphQL", "PostgreSQL", "Redis", "Docker", "Kubernetes",
    "TypeScript", "Rust", "Python", "React", "WebSocket", "gRPC",
    "JWT", "OAuth", "CORS", "SSL", "DNS", "CDN", "S3", "Lambda",
    "Terraform", "Ansible", "GitHub Actions", "Prometheus", "Grafana",
    "Elasticsearch", "RabbitMQ", "Kafka", "Nginx", "HAProxy",
]


@dataclass
class BenchmarkResult:
    token_count: int
    chunk_count: int
    query_latency_p50_ms: float
    query_latency_p95_ms: float
    query_latency_p99_ms: float
    query_latency_mean_ms: float
    index_size_mb: Optional[float] = None


def generate_chunk_content(topic: str, idx: int) -> str:
    """Generate a realistic ~1500 char chunk about a given topic."""
    terms = random.sample(TECH_TERMS, min(5, len(TECH_TERMS)))
    sentences = []

    templates = [
        f"In session {idx}, we discussed how {topic} relates to {terms[0]} and {terms[1]}.",
        f"The team decided to implement {topic} using {terms[2]} as the primary approach.",
        f"Key considerations for {topic} include performance, security, and maintainability.",
        f"When working with {terms[3]}, the {topic} layer needs careful error handling.",
        f"The {terms[4]} integration with {topic} was completed in the last sprint.",
        f"Testing the {topic} module revealed issues with {terms[0]} compatibility.",
        f"Documentation for {topic} was updated to reflect the new {terms[1]} patterns.",
        f"Performance benchmarks for {topic} showed improvements after {terms[2]} optimization.",
        f"The architecture decision record for {topic} was approved by the team.",
        f"Monitoring {topic} in production uses {terms[3]} dashboards and {terms[4]} alerts.",
        f"Security review of {topic} identified no critical issues with the {terms[0]} flow.",
        f"The {topic} refactoring reduced code complexity by consolidating {terms[1]} usage.",
    ]

    # Build content up to ~1500 chars
    random.shuffle(templates)
    content = ""
    for t in templates:
        if len(content) + len(t) + 1 > CHARS_PER_CHUNK:
            break
        content += t + " "

    # Pad if needed
    while len(content) < CHARS_PER_CHUNK - 100:
        extra = random.choice(templates)
        if len(content) + len(extra) + 1 > CHARS_PER_CHUNK:
            break
        content += extra + " "

    return content.strip()


def store_chunks(server: str, chunks: list[str], batch_size: int = 50) -> float:
    """Store chunks via the server API. Returns total time in seconds."""
    total_time = 0
    for i in range(0, len(chunks), batch_size):
        batch = chunks[i:i + batch_size]
        text = "\n\n".join(batch)

        start = time.time()
        try:
            resp = requests.post(
                f"{server}/v1/store",
                json={
                    "text": text,
                    "session_id": f"scale-bench-{i // batch_size}",
                    "source_integration": "benchmark",
                },
                timeout=120,
            )
            resp.raise_for_status()
        except requests.exceptions.RequestException as e:
            print(f"  Store failed at batch {i // batch_size}: {e}", file=sys.stderr)
            continue

        elapsed = time.time() - start
        total_time += elapsed

        if (i // batch_size) % 100 == 0 and i > 0:
            print(f"  Stored {i + len(batch)} chunks ({(i + len(batch)) * TOKENS_PER_CHUNK:,} tokens)...")

    # Flush
    try:
        requests.post(f"{server}/v1/store", json={"text": "", "flush": True}, timeout=30)
    except:
        pass

    return total_time


def measure_query_latency(server: str, queries: list[str], runs_per_query: int = 3) -> list[float]:
    """Run queries and return list of latencies in ms."""
    latencies = []

    for q in queries:
        for _ in range(runs_per_query):
            start = time.time()
            try:
                resp = requests.post(
                    f"{server}/v1/retrieve",
                    json={"query": q, "top_k": 10},
                    timeout=30,
                )
                resp.raise_for_status()
            except requests.exceptions.RequestException as e:
                print(f"  Query failed: {e}", file=sys.stderr)
                continue

            elapsed_ms = (time.time() - start) * 1000
            latencies.append(elapsed_ms)

    return latencies


def percentile(data: list[float], p: float) -> float:
    """Calculate the p-th percentile of a list."""
    if not data:
        return 0
    sorted_data = sorted(data)
    idx = (p / 100) * (len(sorted_data) - 1)
    lower = int(math.floor(idx))
    upper = int(math.ceil(idx))
    if lower == upper:
        return sorted_data[lower]
    frac = idx - lower
    return sorted_data[lower] * (1 - frac) + sorted_data[upper] * frac


def get_index_size(server: str) -> Optional[float]:
    """Get index size in MB from status endpoint."""
    try:
        resp = requests.get(f"{server}/v1/status", timeout=5)
        data = resp.json()
        index_path = data.get("index_path", "")
        if index_path and os.path.exists(index_path):
            total = 0
            for dirpath, _, filenames in os.walk(index_path):
                for f in filenames:
                    total += os.path.getsize(os.path.join(dirpath, f))
            return total / (1024 * 1024)
    except:
        pass
    return None


def run_benchmark(server: str, target_tokens: list[int]) -> list[BenchmarkResult]:
    """Run the full scale benchmark."""
    results = []
    current_chunks = 0

    print(f"Scale Benchmark: query latency vs context space size")
    print(f"Server: {server}")
    print(f"Chunk size: {CHARS_PER_CHUNK} chars ≈ {TOKENS_PER_CHUNK} tokens")
    print(f"Target sizes: {', '.join(format_tokens(t) for t in target_tokens)}")
    print()

    for target in target_tokens:
        target_chunks = target // TOKENS_PER_CHUNK
        chunks_needed = target_chunks - current_chunks

        if chunks_needed <= 0:
            continue

        print(f"--- {format_tokens(target)} ({target_chunks:,} chunks) ---")

        # Generate and store chunks
        print(f"  Generating {chunks_needed:,} chunks...")
        new_chunks = []
        for i in range(chunks_needed):
            topic = random.choice(TOPICS)
            content = generate_chunk_content(topic, current_chunks + i)
            new_chunks.append(content)

        print(f"  Storing...")
        store_time = store_chunks(server, new_chunks)
        current_chunks += chunks_needed
        print(f"  Stored in {store_time:.1f}s")

        # Wait for indexing to settle
        time.sleep(2)

        # Measure query latency
        print(f"  Measuring query latency (10 queries x 3 runs)...")
        latencies = measure_query_latency(server, TEST_QUERIES, runs_per_query=3)

        if not latencies:
            print(f"  No successful queries!")
            continue

        result = BenchmarkResult(
            token_count=current_chunks * TOKENS_PER_CHUNK,
            chunk_count=current_chunks,
            query_latency_p50_ms=percentile(latencies, 50),
            query_latency_p95_ms=percentile(latencies, 95),
            query_latency_p99_ms=percentile(latencies, 99),
            query_latency_mean_ms=sum(latencies) / len(latencies),
            index_size_mb=get_index_size(server),
        )
        results.append(result)

        size_str = f"{result.index_size_mb:.1f}MB" if result.index_size_mb else "?"
        print(f"  p50={result.query_latency_p50_ms:.0f}ms  "
              f"p95={result.query_latency_p95_ms:.0f}ms  "
              f"p99={result.query_latency_p99_ms:.0f}ms  "
              f"mean={result.query_latency_mean_ms:.0f}ms  "
              f"index={size_str}")
        print()

    return results


def format_tokens(n: int) -> str:
    """Format token count as human-readable string."""
    if n >= 1_000_000_000:
        return f"{n / 1_000_000_000:.0f}B"
    if n >= 1_000_000:
        return f"{n / 1_000_000:.0f}M"
    if n >= 1_000:
        return f"{n / 1_000:.0f}K"
    return str(n)


def print_results_table(results: list[BenchmarkResult]):
    """Print results as a markdown table."""
    print()
    print("## Results")
    print()
    print("| Context Space | Chunks | p50 (ms) | p95 (ms) | p99 (ms) | Mean (ms) | Index Size |")
    print("|--------------|--------|----------|----------|----------|-----------|------------|")
    for r in results:
        size = f"{r.index_size_mb:.1f} MB" if r.index_size_mb else "—"
        print(f"| {format_tokens(r.token_count)} tokens | "
              f"{r.chunk_count:,} | "
              f"{r.query_latency_p50_ms:.0f} | "
              f"{r.query_latency_p95_ms:.0f} | "
              f"{r.query_latency_p99_ms:.0f} | "
              f"{r.query_latency_mean_ms:.0f} | "
              f"{size} |")


def save_results(results: list[BenchmarkResult], path: str):
    """Save results to JSON."""
    data = [
        {
            "token_count": r.token_count,
            "chunk_count": r.chunk_count,
            "query_latency_p50_ms": r.query_latency_p50_ms,
            "query_latency_p95_ms": r.query_latency_p95_ms,
            "query_latency_p99_ms": r.query_latency_p99_ms,
            "query_latency_mean_ms": r.query_latency_mean_ms,
            "index_size_mb": r.index_size_mb,
        }
        for r in results
    ]
    with open(path, "w") as f:
        json.dump(data, f, indent=2)
    print(f"\nResults saved to {path}")


def main():
    parser = argparse.ArgumentParser(description="Context Space Scale Benchmark")
    parser.add_argument("--server", default="http://127.0.0.1:8090", help="Server URL")
    parser.add_argument("--max-tokens", type=int, default=500_000_000, help="Maximum token target")
    parser.add_argument("--output", default="tests/scale/results.json", help="Output JSON path")
    parser.add_argument("--quick", action="store_true", help="Quick mode (fewer tiers)")
    args = parser.parse_args()

    # Define scale tiers
    if args.quick:
        tiers = [1_000, 10_000, 100_000, 1_000_000, 10_000_000]
    else:
        tiers = [
            1_000,           # 1K tokens (~3 chunks)
            10_000,          # 10K tokens (~27 chunks)
            100_000,         # 100K tokens (~267 chunks)
            1_000_000,       # 1M tokens (~2,667 chunks)
            5_000_000,       # 5M tokens (~13,333 chunks)
            10_000_000,      # 10M tokens (~26,667 chunks)
            50_000_000,      # 50M tokens (~133,333 chunks)
            100_000_000,     # 100M tokens (~266,667 chunks)
            250_000_000,     # 250M tokens (~666,667 chunks)
            500_000_000,     # 500M tokens (~1,333,333 chunks)
        ]

    # Filter to max
    tiers = [t for t in tiers if t <= args.max_tokens]

    # Check server is up
    try:
        resp = requests.get(f"{args.server}/health", timeout=5)
        resp.raise_for_status()
    except:
        print(f"Error: Server not reachable at {args.server}", file=sys.stderr)
        sys.exit(1)

    results = run_benchmark(args.server, tiers)
    print_results_table(results)
    save_results(results, args.output)


if __name__ == "__main__":
    main()
