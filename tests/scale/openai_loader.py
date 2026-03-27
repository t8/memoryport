#!/usr/bin/env python3
"""
Fast chunk loader using OpenAI embeddings API (~3000 embeddings/sec).
Inserts directly into LanceDB for benchmarking query latency at scale.

Requires: OPENAI_API_KEY environment variable

Usage:
    PYTHONUNBUFFERED=1 python3 tests/scale/openai_loader.py --target-chunks 1333333
"""

import argparse
import json
import os
import random
import sys
import time
import uuid
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime

import requests

OPENAI_URL = "https://api.openai.com/v1/embeddings"
EMBED_MODEL = "text-embedding-3-small"
EMBED_DIM = 768  # Match existing index dimensions
CHARS_PER_CHUNK = 1500
# OpenAI supports up to 2048 inputs per request, but rate limits apply
EMBED_BATCH = 500
PARALLEL_REQUESTS = 4  # Tier 3: 5M TPM

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
]


def log(msg):
    ts = datetime.now().strftime("%H:%M:%S")
    print(f"[{ts}] {msg}", flush=True)


def generate_chunks(count, start_idx):
    chunks = []
    session_counter = start_idx // 20
    for i in range(count):
        idx = start_idx + i
        topic = random.choice(TOPICS)
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

        if i % 20 == 0:
            session_counter += 1

        chunks.append({
            "content": content.strip(),
            "chunk_id": str(uuid.uuid4()),
            "session_id": f"scale-{session_counter}",
            "chunk_type": "conversation",
            "role": random.choice(["user", "assistant"]),
            "timestamp": int(time.time() * 1000) - random.randint(0, 86400000 * 90),
            "arweave_tx_id": f"local_scale_{idx}",
            "batch_index": i % 50,
            "token_count": len(content) // 4,
        })
    return chunks


def embed_openai(texts, api_key, retries=5):
    """Embed texts via OpenAI API. Returns list of 768d vectors."""
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }
    for attempt in range(retries):
        try:
            r = requests.post(OPENAI_URL, headers=headers, json={
                "model": EMBED_MODEL,
                "input": texts,
                "dimensions": EMBED_DIM,
            }, timeout=120)

            if r.status_code == 429:
                # Rate limited — back off
                wait = min(2 ** attempt, 30)
                log(f"  Rate limited, waiting {wait}s...")
                time.sleep(wait)
                continue

            if not r.ok:
                log(f"  API error {r.status_code}: {r.text[:200]}")
                r.raise_for_status()
            data = r.json()
            # Sort by index to maintain order
            embeddings = sorted(data["data"], key=lambda x: x["index"])
            return [e["embedding"] for e in embeddings]

        except Exception as e:
            if attempt < retries - 1:
                time.sleep(2 ** attempt)
                log(f"  Retry {attempt + 1}: {e}")
            else:
                raise


def insert_batch(table, chunks, vectors):
    import pyarrow as pa

    vector_data = pa.FixedSizeListArray.from_arrays(
        pa.array([v for vec in vectors for v in vec], type=pa.float32()),
        EMBED_DIM,
    )
    arrays = {
        "vector": vector_data,
        "chunk_id": pa.array([c["chunk_id"] for c in chunks]),
        "session_id": pa.array([c["session_id"] for c in chunks]),
        "chunk_type": pa.array([c["chunk_type"] for c in chunks]),
        "role": pa.array([c["role"] for c in chunks]),
        "user_id": pa.array(["default"] * len(chunks)),
        "timestamp": pa.array([c["timestamp"] for c in chunks], type=pa.int64()),
        "content": pa.array([c["content"] for c in chunks]),
        "arweave_tx_id": pa.array([c["arweave_tx_id"] for c in chunks]),
        "batch_index": pa.array([c["batch_index"] for c in chunks], type=pa.uint32()),
        "token_count": pa.array([c["token_count"] for c in chunks], type=pa.uint32()),
        "metadata_json": pa.array([json.dumps({"source_integration": "benchmark"})] * len(chunks)),
    }
    table.add(pa.RecordBatch.from_pydict(arrays))


def embed_and_insert_batch(batch_id, chunks, api_key, table):
    """Embed + insert a single batch. Used by thread pool."""
    texts = [c["content"] for c in chunks]
    vectors = embed_openai(texts, api_key)
    insert_batch(table, chunks, vectors)
    return len(chunks)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target-chunks", type=int, default=1_333_333)
    parser.add_argument("--index-path", default=os.path.expanduser("~/.memoryport/index"))
    parser.add_argument("--parallel", type=int, default=PARALLEL_REQUESTS)
    args = parser.parse_args()

    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        log("ERROR: Set OPENAI_API_KEY environment variable")
        sys.exit(1)

    import lancedb
    import pyarrow as pa

    db = lancedb.connect(args.index_path)
    table = db.open_table("chunks")
    current = table.count_rows()
    needed = args.target_chunks - current

    if needed <= 0:
        log(f"Already at {current:,} chunks!")
        return

    log(f"Current:  {current:,} chunks ({current * 375 / 1e6:.0f}M tokens)")
    log(f"Target:   {args.target_chunks:,} chunks ({args.target_chunks * 375 / 1e6:.0f}M tokens)")
    log(f"Need:     {needed:,} chunks")
    log(f"Model:    {EMBED_MODEL} ({EMBED_DIM}d)")
    log(f"Batch:    {EMBED_BATCH} per request, {args.parallel} parallel")
    log(f"")

    inserted = 0
    start_time = time.time()
    executor = ThreadPoolExecutor(max_workers=args.parallel)

    while inserted < needed:
        # Prepare parallel batches
        futures = []
        batch_total = 0

        for _ in range(args.parallel):
            remaining = needed - inserted - batch_total
            if remaining <= 0:
                break
            batch_size = min(EMBED_BATCH, remaining)
            chunks = generate_chunks(batch_size, current + inserted + batch_total)
            future = executor.submit(embed_and_insert_batch, len(futures), chunks, api_key, table)
            futures.append(future)
            batch_total += batch_size

        # Wait for all parallel batches
        for future in as_completed(futures):
            try:
                count = future.result()
                inserted += count
            except Exception as e:
                log(f"  Batch failed: {e}")

        time.sleep(0.1)  # Tier 3: minimal delay

        # Progress
        elapsed = time.time() - start_time
        rate = inserted / elapsed if elapsed > 0 else 0
        eta = (needed - inserted) / rate if rate > 0 else 0
        total = current + inserted
        pct = inserted / needed * 100
        log(f"  {inserted:,}/{needed:,} ({pct:.1f}%) | {total:,} total "
            f"({total * 375 / 1e6:.0f}M tokens) | {rate:.0f}/s | ETA {eta/60:.0f}m")

    elapsed = time.time() - start_time
    total = current + inserted
    log(f"\nDone! {inserted:,} chunks in {elapsed/60:.1f}m ({inserted/elapsed:.0f}/s)")
    log(f"Total: {total:,} chunks ({total * 375 / 1e6:.0f}M tokens)")
    log(f"\nNext: run compaction + benchmark:")
    log(f"  curl -X POST http://localhost:8090/v1/compact")
    log(f"  python3 tests/scale/query_bench.py")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        log("\nInterrupted")
    except Exception as e:
        log(f"Error: {e}")
        import traceback
        traceback.print_exc()
