#!/usr/bin/env python3
"""
Fast chunk loader — bypasses the server API and inserts directly into LanceDB
with pre-computed embeddings from Ollama. For benchmarking query performance
at scale where ingestion path doesn't matter.

Usage:
    PYTHONUNBUFFERED=1 python3 tests/scale/fast_loader.py --target-chunks 1333333
"""

import argparse
import json
import math
import os
import random
import sys
import time
import uuid
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime

import requests

# ── Config ──
OLLAMA_URL = "http://localhost:11434/api/embed"
EMBED_MODEL = "nomic-embed-text"
EMBED_DIM = 768
CHARS_PER_CHUNK = 1500
TOKENS_PER_CHUNK = 375
BATCH_SIZE = 200  # chunks per embedding batch
INSERT_BATCH = 1000  # chunks per LanceDB insert

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


def generate_chunk(idx):
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
    return content.strip()


def embed_batch(texts):
    """Get embeddings from Ollama in one call."""
    r = requests.post(OLLAMA_URL, json={
        "model": EMBED_MODEL,
        "input": texts,
    }, timeout=120)
    r.raise_for_status()
    return r.json()["embeddings"]


def get_current_count():
    """Get current chunk count from LanceDB via server."""
    try:
        r = requests.get("http://localhost:8090/v1/status", timeout=5)
        return r.json().get("indexed_chunks", 0)
    except:
        return 0


def main():
    parser = argparse.ArgumentParser(description="Fast chunk loader for benchmarking")
    parser.add_argument("--target-chunks", type=int, default=1_333_333,
                        help="Target total chunk count (default: 1.33M = 500M tokens)")
    parser.add_argument("--index-path", default=os.path.expanduser("~/.memoryport/index"))
    args = parser.parse_args()

    try:
        import lancedb
        import pyarrow as pa
    except ImportError:
        log("Installing lancedb and pyarrow...")
        os.system(f"{sys.executable} -m pip install lancedb pyarrow")
        import lancedb
        import pyarrow as pa

    # Open LanceDB directly
    db = lancedb.connect(args.index_path)
    table = db.open_table("chunks")

    current = table.count_rows()
    needed = args.target_chunks - current
    target_tokens = args.target_chunks * TOKENS_PER_CHUNK

    log(f"Current: {current:,} chunks ({current * TOKENS_PER_CHUNK / 1e6:.0f}M tokens)")
    log(f"Target:  {args.target_chunks:,} chunks ({target_tokens / 1e6:.0f}M tokens)")
    log(f"Need:    {needed:,} chunks")
    log(f"Batch:   {BATCH_SIZE} embed, {INSERT_BATCH} insert")
    log("")

    if needed <= 0:
        log("Already at target!")
        return

    inserted = 0
    start_time = time.time()
    session_counter = 0

    while inserted < needed:
        batch_count = min(INSERT_BATCH, needed - inserted)

        # Generate chunks
        chunks = []
        for i in range(batch_count):
            idx = current + inserted + i
            content = generate_chunk(idx)
            session_id = f"fast-{session_counter}"
            if i % 20 == 0:
                session_counter += 1
                session_id = f"fast-{session_counter}"

            chunks.append({
                "content": content,
                "chunk_id": str(uuid.uuid4()),
                "session_id": session_id,
                "chunk_type": "conversation",
                "role": random.choice(["user", "assistant"]),
                "timestamp": int(time.time() * 1000) - random.randint(0, 86400000 * 30),
                "arweave_tx_id": f"local_fast_{idx}",
                "batch_index": i % 50,
                "token_count": len(content) // 4,
                "metadata_json": json.dumps({
                    "source_integration": "benchmark",
                    "source_model": None,
                }),
            })

        # Embed in sub-batches
        all_vectors = []
        for j in range(0, len(chunks), BATCH_SIZE):
            sub = chunks[j:j + BATCH_SIZE]
            texts = [c["content"] for c in sub]
            try:
                vecs = embed_batch(texts)
                all_vectors.extend(vecs)
            except Exception as e:
                log(f"  Embed failed: {e}, retrying...")
                time.sleep(2)
                try:
                    vecs = embed_batch(texts)
                    all_vectors.extend(vecs)
                except Exception as e2:
                    log(f"  Embed retry failed: {e2}, skipping batch")
                    break

        if len(all_vectors) != len(chunks):
            log(f"  Vector count mismatch: {len(all_vectors)} vs {len(chunks)}, trimming")
            chunks = chunks[:len(all_vectors)]

        # Build Arrow arrays
        try:
            vector_data = pa.FixedSizeListArray.from_arrays(
                pa.array([v for vec in all_vectors for v in vec], type=pa.float32()),
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
                "metadata_json": pa.array([c["metadata_json"] for c in chunks]),
            }

            record_batch = pa.RecordBatch.from_pydict(arrays)
            table.add(record_batch)
            inserted += len(chunks)

        except Exception as e:
            log(f"  Insert failed: {e}")
            time.sleep(2)
            continue

        # Progress
        elapsed = time.time() - start_time
        rate = inserted / elapsed if elapsed > 0 else 0
        eta = (needed - inserted) / rate if rate > 0 else 0
        total = current + inserted
        log(f"  {inserted:,}/{needed:,} added | {total:,} total ({total * TOKENS_PER_CHUNK / 1e6:.0f}M tokens) | {rate:.0f}/s | ETA {eta/60:.0f}m")

    elapsed = time.time() - start_time
    total = current + inserted
    log(f"\nDone! {inserted:,} chunks added in {elapsed/60:.1f}m ({inserted/elapsed:.0f}/s)")
    log(f"Total: {total:,} chunks ({total * TOKENS_PER_CHUNK / 1e6:.0f}M tokens)")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        log("\nInterrupted")
    except Exception as e:
        log(f"Error: {e}")
        import traceback
        traceback.print_exc()
