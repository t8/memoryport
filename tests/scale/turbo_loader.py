#!/usr/bin/env python3
"""
Turbo chunk loader — maximizes throughput with huge Ollama batches
and pipelined embed/insert. Targets 1hr for 1M+ chunks on M1 Max.

Usage:
    PYTHONUNBUFFERED=1 python3 tests/scale/turbo_loader.py --target-chunks 1333333
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

OLLAMA_URL = "http://localhost:11434/api/embed"
EMBED_MODEL = "nomic-embed-text"
EMBED_DIM = 768
CHARS_PER_CHUNK = 1500
EMBED_BATCH = 5000  # texts per Ollama call (bigger = faster on M1 Max)
INSERT_BATCH = 5000  # match embed batch for simplicity

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
            "session_id": f"turbo-{session_counter}",
            "chunk_type": "conversation",
            "role": random.choice(["user", "assistant"]),
            "timestamp": int(time.time() * 1000) - random.randint(0, 86400000 * 90),
            "arweave_tx_id": f"local_turbo_{idx}",
            "batch_index": i % 50,
            "token_count": len(content) // 4,
        })
    return chunks


def embed_batch(texts):
    r = requests.post(OLLAMA_URL, json={
        "model": EMBED_MODEL,
        "input": texts,
    }, timeout=600)
    r.raise_for_status()
    return r.json()["embeddings"]


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
        "metadata_json": pa.array([json.dumps({"source_integration": "benchmark", "source_model": None})] * len(chunks)),
    }
    table.add(pa.RecordBatch.from_pydict(arrays))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target-chunks", type=int, default=1_333_333)
    parser.add_argument("--index-path", default=os.path.expanduser("~/.memoryport/index"))
    args = parser.parse_args()

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
    log(f"Strategy: {EMBED_BATCH} texts/batch, pipeline embed+insert")
    log(f"")

    inserted = 0
    start_time = time.time()

    # Pipeline: generate next batch while current batch is embedding
    executor = ThreadPoolExecutor(max_workers=2)

    # Pre-generate first batch
    next_chunks = generate_chunks(min(EMBED_BATCH, needed), current)

    while inserted < needed:
        batch_chunks = next_chunks
        batch_size = len(batch_chunks)

        # Start generating next batch in background
        remaining = needed - inserted - batch_size
        if remaining > 0:
            next_count = min(EMBED_BATCH, remaining)
            next_future = executor.submit(generate_chunks, next_count, current + inserted + batch_size)
        else:
            next_future = None

        # Embed current batch
        texts = [c["content"] for c in batch_chunks]
        try:
            vectors = embed_batch(texts)
        except Exception as e:
            log(f"  Embed failed: {e}, retrying...")
            time.sleep(3)
            try:
                vectors = embed_batch(texts)
            except Exception as e2:
                log(f"  Embed retry failed: {e2}, skipping")
                if next_future:
                    next_chunks = next_future.result()
                continue

        # Insert
        try:
            insert_batch(table, batch_chunks, vectors)
            inserted += batch_size
        except Exception as e:
            log(f"  Insert failed: {e}")
            if next_future:
                next_chunks = next_future.result()
            continue

        # Progress
        elapsed = time.time() - start_time
        rate = inserted / elapsed if elapsed > 0 else 0
        eta = (needed - inserted) / rate if rate > 0 else 0
        total = current + inserted
        pct = inserted / needed * 100
        log(f"  {inserted:,}/{needed:,} ({pct:.1f}%) | {total:,} total "
            f"({total * 375 / 1e6:.0f}M tokens) | {rate:.0f}/s | ETA {eta/60:.0f}m")

        # Get next batch
        if next_future:
            next_chunks = next_future.result()
        else:
            break

    elapsed = time.time() - start_time
    total = current + inserted
    log(f"\nDone! {inserted:,} chunks in {elapsed/60:.1f}m ({inserted/elapsed:.0f}/s)")
    log(f"Total: {total:,} chunks ({total * 375 / 1e6:.0f}M tokens)")
    log(f"\nRun compaction: curl -X POST http://localhost:8090/v1/compact")
    log(f"Run benchmark:  python3 tests/scale/query_bench.py")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        log("\nInterrupted")
    except Exception as e:
        log(f"Error: {e}")
        import traceback
        traceback.print_exc()
