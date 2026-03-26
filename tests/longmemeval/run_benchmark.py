#!/usr/bin/env python3
"""
LongMemEval benchmark for Memoryport.

Evaluates retrieval quality by ingesting haystack sessions per question,
querying with the evaluation question, and checking if the correct
answer-containing sessions were retrieved.

Usage:
    python3 tests/longmemeval/run_benchmark.py [--questions 50] [--server http://127.0.0.1:8090] [--dataset oracle]
"""

import argparse
import json
import os
import shutil
import statistics
import sys
import time
import requests
from datetime import datetime

SERVER = "http://127.0.0.1:8090"
MEMORYPORT_INDEX = os.path.expanduser("~/.memoryport/index")


def ingest_haystack(question: dict, server: str) -> int:
    """Ingest all haystack sessions for a question into Memoryport."""
    stored = 0
    for session_id, session_date, session_turns in zip(
        question["haystack_session_ids"],
        question["haystack_dates"],
        question["haystack_sessions"],
    ):
        for turn in session_turns:
            try:
                resp = requests.post(
                    f"{server}/v1/store",
                    json={
                        "text": turn["content"],
                        "chunk_type": "conversation",
                        "session_id": f"{question['question_id']}_{session_id}",
                        "role": turn["role"],
                    },
                    timeout=30,
                )
                if resp.status_code == 200:
                    stored += 1
            except Exception:
                pass
    return stored


def query_retrieval(question: dict, server: str, top_k: int = 50) -> dict:
    """Query Memoryport and check if answer sessions were retrieved."""
    qid = question["question_id"]
    query_text = question["question"]

    start = time.time()
    try:
        resp = requests.post(
            f"{server}/v1/retrieve",
            json={"query": query_text, "top_k": top_k},
            timeout=60,
        )
        latency_ms = (time.time() - start) * 1000

        if resp.status_code != 200:
            return {"qid": qid, "error": f"HTTP {resp.status_code}", "latency_ms": latency_ms}

        results = resp.json().get("results", [])
    except Exception as e:
        return {"qid": qid, "error": str(e), "latency_ms": 0}

    # Extract retrieved session IDs (strip the question_id prefix)
    retrieved_sessions = set()
    for r in results:
        sid = r.get("session_id", "")
        # Session IDs are formatted as "{question_id}_{original_session_id}"
        if "_" in sid:
            original_sid = sid.split("_", 1)[1]
            retrieved_sessions.add(original_sid)

    # Check if answer sessions were retrieved
    answer_sessions = set(question.get("answer_session_ids", []))
    hits = answer_sessions & retrieved_sessions
    recall = len(hits) / len(answer_sessions) if answer_sessions else 0.0

    # Also check turn-level: do any retrieved chunks contain the answer text?
    answer_text = question.get("answer", "").lower()
    content_hit = any(
        answer_text in r.get("content", "").lower()
        for r in results
    )

    return {
        "qid": qid,
        "question_type": question["question_type"],
        "session_recall": recall,
        "content_hit": content_hit,
        "retrieved_sessions": len(retrieved_sessions),
        "answer_sessions": len(answer_sessions),
        "hits": len(hits),
        "latency_ms": latency_ms,
        "num_results": len(results),
    }


def clear_index():
    """Clear the Memoryport index for a fresh ingestion."""
    if os.path.exists(MEMORYPORT_INDEX):
        shutil.rmtree(MEMORYPORT_INDEX)
        os.makedirs(MEMORYPORT_INDEX)


def main():
    parser = argparse.ArgumentParser(description="LongMemEval benchmark for Memoryport")
    parser.add_argument("--questions", type=int, default=50, help="Number of questions to evaluate")
    parser.add_argument("--server", type=str, default=SERVER, help="Memoryport server URL")
    parser.add_argument("--dataset", type=str, default="oracle", choices=["oracle", "s"],
                        help="Dataset variant: oracle (evidence-only) or s (standard)")
    parser.add_argument("--top-k", type=int, default=50, help="Top-K results for retrieval")
    parser.add_argument("--skip-ingest", action="store_true", help="Skip ingestion (use existing index)")
    args = parser.parse_args()

    # Load dataset
    dataset_file = f"tests/longmemeval/data/longmemeval_{args.dataset}_cleaned.json" if args.dataset == "s" else "tests/longmemeval/data/longmemeval_oracle.json"
    if not os.path.exists(dataset_file):
        # Try relative to script
        dataset_file = f"data/longmemeval_{args.dataset}_cleaned.json" if args.dataset == "s" else "data/longmemeval_oracle.json"

    print(f"Loading dataset: {dataset_file}")
    with open(dataset_file) as f:
        all_questions = json.load(f)

    questions = all_questions[: args.questions]
    print(f"Evaluating {len(questions)} questions (dataset={args.dataset}, top_k={args.top_k})")

    if not args.skip_ingest:
        print(f"\nIngesting haystacks...")
        total_stored = 0
        for i, q in enumerate(questions):
            turns = sum(len(s) for s in q["haystack_sessions"])
            stored = ingest_haystack(q, args.server)
            total_stored += stored
            if (i + 1) % 5 == 0:
                print(f"  [{i+1}/{len(questions)}] Ingested {total_stored} turns total")
        print(f"  Total ingested: {total_stored} turns")

    print(f"\nRunning retrieval evaluation...")
    results = []
    for i, q in enumerate(questions):
        result = query_retrieval(q, args.server, args.top_k)
        results.append(result)
        if (i + 1) % 10 == 0:
            recalls = [r["session_recall"] for r in results if "session_recall" in r]
            avg = statistics.mean(recalls) if recalls else 0
            print(f"  [{i+1}/{len(questions)}] Running avg session recall: {avg:.2%}")

    # Aggregate metrics
    print(f"\n{'='*60}")
    print(f"LONGMEMEVAL RESULTS ({len(results)} questions, dataset={args.dataset})")
    print(f"{'='*60}")

    valid = [r for r in results if "error" not in r]
    errors = [r for r in results if "error" in r]

    if valid:
        recalls = [r["session_recall"] for r in valid]
        content_hits = [r["content_hit"] for r in valid]
        latencies = [r["latency_ms"] for r in valid]

        print(f"\nSession-Level Recall: {statistics.mean(recalls):.2%}")
        print(f"  Perfect recall (1.0): {sum(1 for r in recalls if r >= 1.0)}/{len(recalls)}")
        print(f"  Partial recall (>0): {sum(1 for r in recalls if r > 0)}/{len(recalls)}")
        print(f"  Zero recall: {sum(1 for r in recalls if r == 0)}/{len(recalls)}")

        print(f"\nContent Hit Rate: {sum(content_hits)}/{len(content_hits)} ({sum(content_hits)/len(content_hits):.2%})")

        print(f"\nLatency:")
        print(f"  p50: {statistics.median(latencies):.0f}ms")
        print(f"  p95: {sorted(latencies)[int(len(latencies)*0.95)]:.0f}ms")
        print(f"  mean: {statistics.mean(latencies):.0f}ms")

        # By question type
        print(f"\nBy Question Type:")
        by_type = {}
        for r in valid:
            t = r["question_type"]
            by_type.setdefault(t, []).append(r)
        for t, rs in sorted(by_type.items()):
            type_recall = statistics.mean([r["session_recall"] for r in rs])
            type_content = sum(r["content_hit"] for r in rs) / len(rs)
            print(f"  {t:30s}  recall={type_recall:.2%}  content_hit={type_content:.2%}  n={len(rs)}")

    if errors:
        print(f"\nErrors: {len(errors)}")
        for e in errors[:3]:
            print(f"  {e['qid']}: {e['error']}")

    # Save detailed results
    output_file = f"tests/longmemeval/results_{args.dataset}_{args.questions}q.json"
    with open(output_file, "w") as f:
        json.dump(results, f, indent=2)
    print(f"\nDetailed results saved to {output_file}")


if __name__ == "__main__":
    main()
