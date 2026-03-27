#!/usr/bin/env python3
"""
500M Token Context Space Benchmark

Grows the index from current size to 500M tokens, measuring query latency
at intermediate checkpoints. Runs compaction at each checkpoint.

Uses the real embedding pipeline (Ollama nomic-embed-text) for intellectual
honesty — no synthetic vectors.

Checkpoints: 100M, 150M, 200M, 250M, 300M, 400M, 500M tokens

Usage:
    PYTHONUNBUFFERED=1 python3 tests/scale/bench_500m.py [--server http://127.0.0.1:8090]
"""

import argparse
import json
import math
import os
import random
import sys
import time
import traceback
from datetime import datetime

import requests

# ── Constants ──
CHARS_PER_CHUNK = 1500
TOKENS_PER_CHUNK = 375
SERVER = "http://127.0.0.1:8090"
OUTPUT_FILE = "tests/scale/results_500m.json"
LOG_FILE = "tests/scale/bench_500m.log"

# Checkpoints in tokens
CHECKPOINTS = [
    100_000_000,   # 100M
    150_000_000,   # 150M
    200_000_000,   # 200M
    250_000_000,   # 250M
    300_000_000,   # 300M
    400_000_000,   # 400M
    500_000_000,   # 500M
]

TOPICS = [
    "authentication", "database", "caching", "deployment", "testing",
    "monitoring", "api-design", "error-handling", "security", "performance",
    "frontend", "backend", "infrastructure", "ci-cd", "documentation",
    "refactoring", "debugging", "code-review", "architecture", "scaling",
    "microservices", "containers", "networking", "storage", "messaging",
    "observability", "incident-response", "compliance", "data-pipeline", "ml-ops",
]

TECH_TERMS = [
    "REST API", "GraphQL", "PostgreSQL", "Redis", "Docker", "Kubernetes",
    "TypeScript", "Rust", "Python", "React", "WebSocket", "gRPC",
    "JWT", "OAuth", "CORS", "SSL", "DNS", "CDN", "S3", "Lambda",
    "Terraform", "Ansible", "GitHub Actions", "Prometheus", "Grafana",
    "Elasticsearch", "RabbitMQ", "Kafka", "Nginx", "HAProxy",
    "LanceDB", "Arweave", "LLM", "embeddings", "vector search",
    "chunking", "retrieval", "context window", "MCP", "proxy",
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


def log(msg):
    ts = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    line = f"[{ts}] {msg}"
    print(line, flush=True)
    with open(LOG_FILE, "a") as f:
        f.write(line + "\n")


def alert(msg):
    """Log an error prominently."""
    log(f"!!! ALERT: {msg}")


def format_tokens(n):
    if n >= 1e9: return f"{n/1e9:.1f}B"
    if n >= 1e6: return f"{n/1e6:.0f}M"
    if n >= 1e3: return f"{n/1e3:.0f}K"
    return str(n)


def get_current_chunks(server):
    """Get current chunk count from server."""
    try:
        r = requests.get(f"{server}/v1/status", timeout=15)
        r.raise_for_status()
        return r.json().get("indexed_chunks", 0)
    except Exception as e:
        alert(f"Failed to get status: {e}")
        return None


def generate_chunk(topic, idx):
    terms = random.sample(TECH_TERMS, 5)
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
    random.shuffle(templates)
    content = ""
    for t in templates:
        if len(content) + len(t) + 1 > CHARS_PER_CHUNK:
            break
        content += t + " "
    return content.strip()


def store_batch(server, chunks, session_id, retries=3):
    """Store a batch of chunks. Returns True on success."""
    text = "\n\n".join(chunks)
    for attempt in range(retries):
        try:
            r = requests.post(
                f"{server}/v1/store",
                json={
                    "text": text,
                    "session_id": session_id,
                    "source_integration": "benchmark",
                },
                timeout=120,
            )
            r.raise_for_status()
            return True
        except Exception as e:
            if attempt < retries - 1:
                log(f"  Store retry {attempt + 1}/{retries}: {e}")
                time.sleep(2 ** attempt)
            else:
                alert(f"Store failed after {retries} retries: {e}")
                return False
    return False


def run_compaction(server):
    """Trigger compaction via the /v1/compact endpoint."""
    log("  Running compaction...")
    try:
        r = requests.post(f"{server}/v1/compact", timeout=300)  # May take minutes at scale
        if r.ok:
            log("  Compaction complete")
            return True
        else:
            alert(f"Compaction returned {r.status_code}: {r.text}")
            return False
    except requests.exceptions.Timeout:
        alert("Compaction timed out after 5 minutes")
        return False
    except Exception as e:
        alert(f"Compaction failed: {e}")
        return False


def measure_latency(server, runs_per_query=5):
    """Run queries and return latency stats."""
    latencies = []

    for q in QUERIES:
        for _ in range(runs_per_query):
            try:
                start = time.time()
                r = requests.post(
                    f"{server}/v1/retrieve",
                    json={"query": q, "top_k": 10},
                    timeout=30,
                )
                r.raise_for_status()
                ms = (time.time() - start) * 1000
                latencies.append(ms)
            except Exception as e:
                alert(f"Query failed: {e}")

    if not latencies:
        return None

    latencies.sort()
    return {
        "p50": latencies[len(latencies) // 2],
        "p95": latencies[int(len(latencies) * 0.95)],
        "p99": latencies[int(len(latencies) * 0.99)],
        "mean": sum(latencies) / len(latencies),
        "min": latencies[0],
        "max": latencies[-1],
        "count": len(latencies),
    }


def get_index_size_mb():
    """Get index size in MB."""
    index_path = os.path.expanduser("~/.memoryport/index")
    if not os.path.exists(index_path):
        return None
    total = 0
    for dirpath, _, filenames in os.walk(index_path):
        for f in filenames:
            total += os.path.getsize(os.path.join(dirpath, f))
    return total / (1024 * 1024)


def save_results(results):
    with open(OUTPUT_FILE, "w") as f:
        json.dump(results, f, indent=2)
    log(f"Results saved to {OUTPUT_FILE}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--server", default=SERVER)
    args = parser.parse_args()
    server = args.server

    log("=" * 60)
    log("500M Token Context Space Benchmark")
    log(f"Server: {server}")
    log(f"Chunk size: {CHARS_PER_CHUNK} chars = {TOKENS_PER_CHUNK} tokens")
    log(f"Checkpoints: {', '.join(format_tokens(c) for c in CHECKPOINTS)}")
    log("=" * 60)

    # Check server
    current_chunks = get_current_chunks(server)
    if current_chunks is None:
        alert("Server not reachable!")
        sys.exit(1)

    current_tokens = current_chunks * TOKENS_PER_CHUNK
    log(f"Starting with {current_chunks:,} chunks ({format_tokens(current_tokens)} tokens)")

    results = []
    batch_size = 50  # chunks per API call
    store_failures = 0
    max_failures = 50  # abort if too many failures

    for checkpoint in CHECKPOINTS:
        target_chunks = checkpoint // TOKENS_PER_CHUNK
        chunks_needed = target_chunks - current_chunks

        if chunks_needed <= 0:
            log(f"\n--- {format_tokens(checkpoint)} --- (already have enough, measuring latency)")
        else:
            log(f"\n--- {format_tokens(checkpoint)} --- (need {chunks_needed:,} more chunks)")

            # Generate and store in batches
            stored = 0
            start_time = time.time()

            while stored < chunks_needed:
                batch = []
                for j in range(min(batch_size, chunks_needed - stored)):
                    topic = random.choice(TOPICS)
                    content = generate_chunk(topic, current_chunks + stored + j)
                    batch.append(content)

                session_id = f"bench500m-{checkpoint // 1_000_000}m-{stored // 5000}"
                ok = store_batch(server, batch, session_id)

                if ok:
                    stored += len(batch)
                    store_failures = 0  # reset on success
                else:
                    store_failures += 1
                    if store_failures >= max_failures:
                        alert(f"Too many store failures ({max_failures}), aborting!")
                        save_results(results)
                        sys.exit(1)

                # Progress every 5K chunks
                if stored % 5000 < batch_size:
                    elapsed = time.time() - start_time
                    rate = stored / elapsed if elapsed > 0 else 0
                    eta = (chunks_needed - stored) / rate if rate > 0 else 0
                    log(f"  {stored:,}/{chunks_needed:,} chunks "
                        f"({rate:.0f}/s, ETA {eta/60:.0f}m)")

            current_chunks += stored
            elapsed = time.time() - start_time
            log(f"  Stored {stored:,} chunks in {elapsed/60:.1f}m ({stored/elapsed:.0f}/s)")

        # Compact before measuring
        log("  Running compaction...")
        if not run_compaction(server):
            alert("Compaction failed! Measuring anyway...")

        # Wait a moment for things to settle
        time.sleep(3)

        # Measure latency
        log(f"  Measuring latency (20 queries x 5 runs)...")
        stats = measure_latency(server, runs_per_query=5)

        if stats is None:
            alert("All queries failed at this checkpoint!")
            continue

        index_mb = get_index_size_mb()

        result = {
            "checkpoint_tokens": checkpoint,
            "actual_chunks": current_chunks,
            "actual_tokens": current_chunks * TOKENS_PER_CHUNK,
            "p50_ms": stats["p50"],
            "p95_ms": stats["p95"],
            "p99_ms": stats["p99"],
            "mean_ms": stats["mean"],
            "min_ms": stats["min"],
            "max_ms": stats["max"],
            "index_size_mb": index_mb,
            "timestamp": datetime.now().isoformat(),
        }
        results.append(result)
        save_results(results)  # Save after each checkpoint

        size_str = f"{index_mb:.0f}MB" if index_mb else "?"
        log(f"  RESULT: p50={stats['p50']:.0f}ms  p95={stats['p95']:.0f}ms  "
            f"mean={stats['mean']:.0f}ms  index={size_str}")

    # Final summary
    log("\n" + "=" * 60)
    log("FINAL RESULTS")
    log("=" * 60)
    log(f"\n| Context Space | Chunks | p50 (ms) | p95 (ms) | Mean (ms) | Index |")
    log(f"|---|---|---|---|---|---|")
    for r in results:
        size = f"{r['index_size_mb']:.0f} MB" if r.get('index_size_mb') else "—"
        log(f"| {format_tokens(r['actual_tokens'])} | {r['actual_chunks']:,} | "
            f"{r['p50_ms']:.0f} | {r['p95_ms']:.0f} | {r['mean_ms']:.0f} | {size} |")

    log("\nBenchmark complete!")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        log("\nBenchmark interrupted by user")
        sys.exit(0)
    except Exception as e:
        alert(f"Unexpected error: {e}")
        traceback.print_exc()
        sys.exit(1)
