#!/usr/bin/env python3
"""
Parallel chunk loader — multiple workers embedding and inserting concurrently.
Each worker embeds via Ollama and inserts directly into LanceDB.

Usage:
    PYTHONUNBUFFERED=1 python3 tests/scale/parallel_loader.py --workers 8 --target-chunks 1333333
"""

import argparse
import json
import os
import random
import sys
import time
import uuid
import threading
from datetime import datetime
from multiprocessing import Process, Value, Lock

import requests

OLLAMA_URL = "http://localhost:11434/api/embed"
EMBED_MODEL = "nomic-embed-text"
EMBED_DIM = 768
CHARS_PER_CHUNK = 1500
EMBED_BATCH = 200
INSERT_BATCH = 500

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


def embed_batch(texts, retries=3):
    for attempt in range(retries):
        try:
            r = requests.post(OLLAMA_URL, json={
                "model": EMBED_MODEL,
                "input": texts,
            }, timeout=120)
            r.raise_for_status()
            return r.json()["embeddings"]
        except Exception as e:
            if attempt < retries - 1:
                time.sleep(1)
            else:
                raise


def worker_fn(worker_id, index_path, start_idx, count, shared_counter, counter_lock):
    """Each worker generates, embeds, and inserts its own slice of chunks."""
    import lancedb
    import pyarrow as pa

    db = lancedb.connect(index_path)
    table = db.open_table("chunks")

    inserted = 0
    offset = start_idx
    batch_num = 0

    while inserted < count:
        batch_count = min(INSERT_BATCH, count - inserted)

        # Generate text
        chunks = []
        for i in range(batch_count):
            idx = offset + inserted + i
            content = generate_chunk(idx)
            chunks.append({
                "content": content,
                "chunk_id": str(uuid.uuid4()),
                "session_id": f"w{worker_id}-{batch_num}",
                "chunk_type": "conversation",
                "role": random.choice(["user", "assistant"]),
                "timestamp": int(time.time() * 1000) - random.randint(0, 86400000 * 90),
                "arweave_tx_id": f"local_w{worker_id}_{idx}",
                "batch_index": i % 50,
                "token_count": len(content) // 4,
                "metadata_json": json.dumps({
                    "source_integration": "benchmark",
                    "source_model": None,
                }),
            })
            if i % 20 == 0:
                batch_num += 1

        # Embed in sub-batches
        all_vectors = []
        ok = True
        for j in range(0, len(chunks), EMBED_BATCH):
            sub = chunks[j:j + EMBED_BATCH]
            texts = [c["content"] for c in sub]
            try:
                vecs = embed_batch(texts)
                all_vectors.extend(vecs)
            except Exception as e:
                log(f"  W{worker_id}: embed failed: {e}")
                ok = False
                break

        if not ok or len(all_vectors) != len(chunks):
            if all_vectors:
                chunks = chunks[:len(all_vectors)]
            else:
                time.sleep(2)
                continue

        # Insert into LanceDB
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
            table.add(pa.RecordBatch.from_pydict(arrays))
            inserted += len(chunks)

            with counter_lock:
                shared_counter.value += len(chunks)

        except Exception as e:
            log(f"  W{worker_id}: insert failed: {e}")
            time.sleep(2)
            continue


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--target-chunks", type=int, default=1_333_333)
    parser.add_argument("--index-path", default=os.path.expanduser("~/.memoryport/index"))
    args = parser.parse_args()

    import lancedb
    db = lancedb.connect(args.index_path)
    table = db.open_table("chunks")
    current = table.count_rows()
    needed = args.target_chunks - current

    if needed <= 0:
        log(f"Already at {current:,} chunks. Target: {args.target_chunks:,}")
        return

    log(f"Current:  {current:,} chunks ({current * 375 / 1e6:.0f}M tokens)")
    log(f"Target:   {args.target_chunks:,} chunks ({args.target_chunks * 375 / 1e6:.0f}M tokens)")
    log(f"Need:     {needed:,} chunks")
    log(f"Workers:  {args.workers}")
    log(f"")

    # Split work across workers
    per_worker = needed // args.workers
    remainder = needed % args.workers

    shared_counter = Value('i', 0)
    counter_lock = Lock()

    processes = []
    for w in range(args.workers):
        count = per_worker + (1 if w < remainder else 0)
        start_idx = current + w * per_worker
        p = Process(target=worker_fn, args=(
            w, args.index_path, start_idx, count,
            shared_counter, counter_lock,
        ))
        processes.append(p)

    start_time = time.time()
    for p in processes:
        p.start()

    # Monitor progress
    while any(p.is_alive() for p in processes):
        time.sleep(10)
        elapsed = time.time() - start_time
        done = shared_counter.value
        rate = done / elapsed if elapsed > 0 else 0
        eta = (needed - done) / rate if rate > 0 else 0
        total = current + done
        pct = done / needed * 100
        log(f"  {done:,}/{needed:,} ({pct:.1f}%) | {total:,} total ({total * 375 / 1e6:.0f}M tokens) | {rate:.0f}/s | ETA {eta/60:.0f}m")

    for p in processes:
        p.join()

    elapsed = time.time() - start_time
    total_inserted = shared_counter.value
    total = current + total_inserted
    log(f"\nDone! {total_inserted:,} chunks in {elapsed/60:.1f}m ({total_inserted/elapsed:.0f}/s)")
    log(f"Total: {total:,} chunks ({total * 375 / 1e6:.0f}M tokens)")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        log("\nInterrupted")
    except Exception as e:
        log(f"Error: {e}")
        import traceback
        traceback.print_exc()
