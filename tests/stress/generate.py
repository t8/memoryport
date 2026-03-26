#!/usr/bin/env python3
"""
Stress test: Generate and store 10K+ realistic conversation chunks.
Simulates months of coding conversations across multiple topics and sessions.

Usage:
    python3 tests/stress/generate.py [--chunks 10000] [--server http://127.0.0.1:8090]
"""

import argparse
import json
import random
import time
import requests
from datetime import datetime, timedelta

SERVER = "http://127.0.0.1:8090"

# Realistic coding conversation topics
TOPICS = {
    "auth": {
        "user": [
            "How should we implement authentication for the API?",
            "Can we use JWT tokens instead of session cookies?",
            "The auth middleware is rejecting valid tokens after restart",
            "We need to add role-based access control to the admin endpoints",
            "How do we handle token refresh without logging the user out?",
            "The OAuth callback is failing with a CORS error",
            "Should we store sessions in Redis or PostgreSQL?",
            "The password hashing is too slow — bcrypt with 12 rounds takes 300ms",
            "We need to add rate limiting to the login endpoint",
            "How do we implement API key authentication alongside JWT?",
        ],
        "assistant": [
            "I'd recommend JWT with short-lived access tokens (15 min) and longer refresh tokens (7 days). Store refresh tokens in the database for revocation.",
            "The issue is that your JWT secret is loaded from an env var that's not set after restart. Add it to your .env file.",
            "For RBAC, add a `role` claim to the JWT payload and a middleware that checks it against route permissions.",
            "The CORS error is because the OAuth provider redirects to a different origin. Add the callback URL to your CORS whitelist.",
            "For API keys, use SHA-256 hashing (not bcrypt — API keys have high entropy). Store the hash, compare on each request.",
        ],
    },
    "database": {
        "user": [
            "The query is taking 3 seconds — how do we optimize it?",
            "Should we add an index on the timestamp column?",
            "The migration failed halfway through — how do we roll back?",
            "We're getting connection pool exhaustion under load",
            "How do we handle the N+1 query problem in the sessions endpoint?",
            "The database is at 90% disk usage — what should we clean up?",
            "Should we shard the chunks table by user_id?",
            "The JSONB query on metadata is slow — should we extract columns?",
            "We need to add a foreign key but the table has 10M rows",
            "How do we do zero-downtime schema migrations?",
        ],
        "assistant": [
            "Add a composite index on (user_id, timestamp) — your queries always filter by user_id first.",
            "Use `BEGIN; ... ROLLBACK;` to undo. If it was DDL, check if your migration tool supports transactional migrations.",
            "Increase the pool size to match your max concurrent requests. Also check for leaked connections — are you closing them in error paths?",
            "Use eager loading or a DataLoader pattern. Batch the session queries into a single IN clause.",
            "For zero-downtime migrations: add the new column as nullable, backfill, then add the NOT NULL constraint.",
        ],
    },
    "frontend": {
        "user": [
            "The React component is re-rendering 50 times on each keystroke",
            "How do we implement dark mode across the whole app?",
            "The bundle size is 2MB — what's the biggest contributor?",
            "Tailwind purge isn't working — all styles are in the bundle",
            "The chart library is causing a memory leak on unmount",
            "How do we handle form validation with React Hook Form?",
            "The WebSocket connection drops every 30 seconds",
            "CSS grid layout breaks on Safari iOS",
            "The image lazy loading isn't working below the fold",
            "How do we implement infinite scroll without a library?",
        ],
        "assistant": [
            "Wrap the expensive computation in useMemo and the callback in useCallback. The parent is likely re-rendering and passing new object references.",
            "Use CSS variables for theming. Define --bg, --text, --border in :root and .dark. Toggle the class on <html>.",
            "Run `npx vite-bundle-visualizer`. Common culprits: moment.js (use dayjs), lodash (use lodash-es), full icon libraries.",
            "Check your tailwind.config.js content paths. They need to match where you actually use Tailwind classes.",
            "For infinite scroll: use IntersectionObserver on a sentinel element at the bottom. When it enters the viewport, fetch the next page.",
        ],
    },
    "deployment": {
        "user": [
            "The Docker build is taking 15 minutes — how do we speed it up?",
            "The container keeps getting OOM killed in production",
            "How do we set up blue-green deployment with zero downtime?",
            "The health check is passing but the app isn't serving requests",
            "We need to rotate the TLS certificates without downtime",
            "The CI pipeline is flaky — tests pass locally but fail in CI",
            "How do we handle secrets in the Docker image?",
            "The Kubernetes pod keeps restarting with CrashLoopBackOff",
            "We need to add horizontal pod autoscaling based on request rate",
            "The database connection string is hardcoded — how do we externalize it?",
        ],
        "assistant": [
            "Use multi-stage builds. First stage compiles, second stage copies only the binary. Also cache cargo dependencies by copying Cargo.toml first.",
            "The OOM is likely from unbounded caches or connection pools. Add memory limits to your container spec and profile with `docker stats`.",
            "For blue-green: deploy the new version alongside the old, run health checks, then switch the load balancer. Rollback = switch back.",
            "Your health check probably returns 200 before the app is fully initialized. Add a readiness probe that checks actual dependencies (DB, cache).",
            "Use environment variables or a secrets manager (Vault, AWS Secrets Manager). Never bake secrets into Docker images.",
        ],
    },
    "debugging": {
        "user": [
            "There's a race condition in the batcher — chunks are being lost",
            "The error says 'connection refused' but the service is running",
            "Memory usage grows linearly and never drops — is this a leak?",
            "The timestamp is wrong by exactly 5 hours — timezone issue?",
            "The API returns 200 but the response body is empty",
            "Logs show 'too many open files' — what's the limit?",
            "The binary works on my machine but segfaults in production",
            "The WebSocket upgrade fails with a 400 Bad Request",
            "CPU usage spikes to 100% every 60 seconds for 5 seconds",
            "The test passes in isolation but fails when run with other tests",
        ],
        "assistant": [
            "The race condition is likely in the flush callback. The lock is released between checking the buffer and writing — another thread can add chunks that get lost.",
            "Connection refused means nothing is listening on that port. Check if the service bound to 0.0.0.0 vs 127.0.0.1, and verify the port matches.",
            "Growing memory without drops usually means you're holding references. Check for global caches, event listeners that aren't removed, or growing vectors.",
            "The 5-hour offset is UTC vs EST. Your server is in UTC but you're formatting with the local timezone. Use UTC consistently.",
            "The 60-second CPU spike is likely a scheduled task — cron job, garbage collection, or your batcher's flush timer. Add logging around timed operations.",
        ],
    },
    "architecture": {
        "user": [
            "Should we use microservices or a monolith for this?",
            "How do we handle eventual consistency between the index and Arweave?",
            "The event bus is becoming a bottleneck — messages are backing up",
            "Should we use gRPC or REST for internal service communication?",
            "How do we implement the saga pattern for distributed transactions?",
            "The cache invalidation strategy is causing stale data issues",
            "Should we use CQRS for the read-heavy query endpoint?",
            "How do we handle schema evolution without breaking old clients?",
            "The monorepo is getting unwieldy — should we split it?",
            "How do we design the API for backward compatibility?",
        ],
        "assistant": [
            "Start with a monolith. Extract services only when you have a clear scaling or organizational reason. Premature microservices are the #1 architecture mistake.",
            "Use a write-ahead log. Write to the local index first (immediate), then async sync to Arweave. On cold start, rebuild from Arweave.",
            "For schema evolution: always add fields (never remove), use optional fields, version your API (v1, v2), and support the old version for at least 6 months.",
            "CQRS makes sense here — the write path (store) and read path (query) have very different performance characteristics and can be optimized independently.",
            "For backward compatibility: never change field types, never rename fields, add new endpoints for new behavior instead of changing existing ones.",
        ],
    },
}

# Code snippets to include in some chunks
CODE_SNIPPETS = [
    '```rust\nfn main() {\n    let config = Config::from_file("config.toml")?;\n    let engine = Engine::new(config).await?;\n    engine.serve().await\n}\n```',
    '```python\ndef process_batch(chunks):\n    embeddings = model.encode([c.content for c in chunks])\n    index.add(embeddings, [c.id for c in chunks])\n```',
    '```typescript\nconst results = await fetch("/api/search", {\n  method: "POST",\n  body: JSON.stringify({ query, topK: 10 }),\n});\n```',
    '```sql\nSELECT s.session_id, COUNT(*) as chunk_count\nFROM chunks\nWHERE user_id = $1\nGROUP BY s.session_id\nORDER BY MAX(timestamp) DESC;\n```',
    '```bash\ncurl -X POST http://localhost:8080/v1/store \\\n  -H "Content-Type: application/json" \\\n  -d \'{"text": "hello world"}\'\n```',
]


def generate_chunks(n: int) -> list:
    """Generate n realistic conversation chunks across topics and sessions."""
    chunks = []
    session_count = 0
    topics = list(TOPICS.keys())
    base_time = datetime.now() - timedelta(days=90)  # Start 90 days ago

    i = 0
    while i < n:
        # Each session is a conversation of 5-20 turns
        topic = random.choice(topics)
        topic_data = TOPICS[topic]
        session_count += 1
        session_id = f"stress-{topic}-{session_count:04d}"
        turns = random.randint(5, 20)
        session_time = base_time + timedelta(
            minutes=random.randint(0, 90 * 24 * 60)
        )

        for t in range(min(turns, n - i)):
            # User message
            user_msg = random.choice(topic_data["user"])
            # Sometimes include code
            if random.random() < 0.3:
                user_msg += "\n\nHere's the relevant code:\n" + random.choice(
                    CODE_SNIPPETS
                )

            chunks.append(
                {
                    "text": user_msg,
                    "session_id": session_id,
                    "chunk_type": "conversation",
                    "role": "user",
                }
            )
            i += 1
            if i >= n:
                break

            # Assistant response
            assistant_msg = random.choice(topic_data["assistant"])
            chunks.append(
                {
                    "text": assistant_msg,
                    "session_id": session_id,
                    "chunk_type": "conversation",
                    "role": "assistant",
                }
            )
            i += 1

    return chunks


def store_chunks(chunks: list, server: str, batch_size: int = 50):
    """Store chunks via the server API."""
    stored = 0
    errors = 0
    start = time.time()

    for chunk in chunks:
        try:
            resp = requests.post(
                f"{server}/v1/store",
                json=chunk,
                timeout=30,
            )
            if resp.status_code == 200:
                stored += 1
            else:
                errors += 1
                if errors <= 3:
                    print(f"  Error: {resp.status_code} {resp.text[:100]}")
        except Exception as e:
            errors += 1
            if errors <= 3:
                print(f"  Exception: {e}")

        if stored % 100 == 0 and stored > 0:
            elapsed = time.time() - start
            rate = stored / elapsed
            print(f"  Stored {stored}/{len(chunks)} ({rate:.1f} chunks/sec)")

    elapsed = time.time() - start
    print(
        f"\nDone: {stored} stored, {errors} errors in {elapsed:.1f}s ({stored/elapsed:.1f} chunks/sec)"
    )
    return stored, errors


def main():
    parser = argparse.ArgumentParser(description="Stress test chunk generator")
    parser.add_argument(
        "--chunks", type=int, default=10000, help="Number of chunks to generate"
    )
    parser.add_argument(
        "--server", type=str, default=SERVER, help="Server URL"
    )
    args = parser.parse_args()

    print(f"Generating {args.chunks} chunks...")
    chunks = generate_chunks(args.chunks)
    print(f"Generated {len(chunks)} chunks across {len(set(c['session_id'] for c in chunks))} sessions")

    print(f"\nStoring to {args.server}...")
    stored, errors = store_chunks(chunks, args.server)

    # Check final status
    try:
        status = requests.get(f"{args.server}/v1/status").json()
        print(f"\nServer status: {status['indexed_chunks']} indexed chunks")
    except:
        pass


if __name__ == "__main__":
    main()
