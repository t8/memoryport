#!/usr/bin/env python3
"""
LongMemEval Answer Accuracy benchmark for Memoryport.

Extends the retrieval benchmark with LLM answer generation + LLM-as-judge
evaluation, producing scores directly comparable to Supermemory's
LongMemEval results.

Pipeline:
  1. Ingest haystack sessions into Memoryport (or skip with --skip-ingest)
  2. Retrieve relevant context for each question
  3. Send context + question to an LLM to generate an answer
  4. Use an LLM judge to compare generated answer to ground truth
  5. Report accuracy by question type

Usage:
    # With Claude as answer model
    python3 tests/longmemeval/run_answer_accuracy.py \
        --questions 50 --dataset oracle \
        --answer-model claude-sonnet-4-20250514

    # With GPT-4o (for comparison with Supermemory)
    python3 tests/longmemeval/run_answer_accuracy.py \
        --questions 50 --dataset oracle \
        --answer-model gpt-4o

    # Skip ingestion if index already loaded
    python3 tests/longmemeval/run_answer_accuracy.py \
        --questions 50 --skip-ingest \
        --answer-model gpt-4o
"""

import argparse
import json
import os
import shutil
import statistics
import sys
import time

import requests

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
    """Query Memoryport and return retrieval results."""
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

    # Session recall
    retrieved_sessions = set()
    for r in results:
        sid = r.get("session_id", "")
        if "_" in sid:
            original_sid = sid.split("_", 1)[1]
            retrieved_sessions.add(original_sid)

    answer_sessions = set(question.get("answer_session_ids", []))
    hits = answer_sessions & retrieved_sessions
    recall = len(hits) / len(answer_sessions) if answer_sessions else 0.0

    return {
        "qid": qid,
        "question_type": question["question_type"],
        "session_recall": recall,
        "hits": len(hits),
        "answer_sessions": len(answer_sessions),
        "retrieved_sessions": len(retrieved_sessions),
        "latency_ms": latency_ms,
        "num_results": len(results),
        "context": [r.get("content", "") for r in results],
    }


def call_anthropic(messages: list, model: str, api_key: str, max_tokens: int = 1024) -> str:
    """Call Anthropic API."""
    resp = requests.post(
        "https://api.anthropic.com/v1/messages",
        headers={
            "x-api-key": api_key,
            "anthropic-version": "2023-06-01",
            "content-type": "application/json",
        },
        json={
            "model": model,
            "max_tokens": max_tokens,
            "messages": messages,
        },
        timeout=120,
    )
    resp.raise_for_status()
    return resp.json()["content"][0]["text"]


def call_openai(messages: list, model: str, api_key: str, max_tokens: int = 1024) -> str:
    """Call OpenAI API."""
    resp = requests.post(
        "https://api.openai.com/v1/chat/completions",
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
        json={
            "model": model,
            "max_tokens": max_tokens,
            "messages": messages,
        },
        timeout=120,
    )
    resp.raise_for_status()
    return resp.json()["choices"][0]["message"]["content"]


def call_llm(messages: list, model: str, max_tokens: int = 1024) -> str:
    """Route to the correct API based on model name."""
    if model.startswith("claude") or model.startswith("anthropic"):
        api_key = os.environ.get("ANTHROPIC_API_KEY")
        if not api_key:
            raise ValueError("ANTHROPIC_API_KEY not set")
        return call_anthropic(messages, model, api_key, max_tokens)
    else:
        api_key = os.environ.get("OPENAI_API_KEY")
        if not api_key:
            raise ValueError("OPENAI_API_KEY not set")
        return call_openai(messages, model, api_key, max_tokens)


def generate_answer(question: str, context: list[str], model: str,
                    question_date: str = None) -> str:
    """Generate an answer using retrieved context + LLM."""
    context_text = "\n\n---\n\n".join(context[:20])  # Limit context to top 20 chunks

    date_line = ""
    if question_date:
        date_line = f"The question was asked on: {question_date}\n\n"

    messages = [
        {
            "role": "user",
            "content": (
                f"You are answering a question based on your conversation history with "
                f"the user. Use the retrieved conversation excerpts below to answer.\n\n"
                f"{date_line}"
                f"Retrieved conversation history:\n{context_text}\n\n"
                f"Question: {question}\n\n"
                f"Answer the question concisely based on the conversation history above. "
                f"Extract all relevant information and reason step by step if needed. "
                f"Pay attention to dates and temporal ordering of events."
            ),
        }
    ]

    return call_llm(messages, model, max_tokens=512)


# Type-specific judge prompts matching MemoryBench methodology
JUDGE_BASE = (
    "I will give you a question, a correct answer, and a response from a model. "
    "Please answer yes if the response contains the correct answer. Otherwise, "
    "answer no. If the response is equivalent to the correct answer or contains "
    "all the intermediate steps to get the correct answer, you should also answer "
    "yes. If the response only contains a subset of the information required by "
    "the answer, answer no."
)

JUDGE_TEMPORAL_EXTRA = (
    " In addition, do not penalize off-by-one errors for the number of days. If "
    "the question asks for the number of days/weeks/months, etc., and the model "
    "makes off-by-one errors (e.g., predicting 19 days when the answer is 18), "
    "the model's response is still correct."
)

JUDGE_KNOWLEDGE_UPDATE_EXTRA = (
    " If the response contains some previous information along with an updated "
    "answer, the response should be considered as correct as long as the updated "
    "answer is the required answer."
)


def judge_answer(question: str, ground_truth: str, predicted: str, model: str,
                 question_type: str = None) -> dict:
    """Use LLM-as-judge to evaluate answer correctness.

    Uses type-specific prompts matching MemoryBench methodology for fair
    comparison with Supermemory and other systems evaluated on that framework.
    """
    instructions = JUDGE_BASE
    if question_type == "temporal-reasoning":
        instructions += JUDGE_TEMPORAL_EXTRA
    elif question_type == "knowledge-update":
        instructions += JUDGE_KNOWLEDGE_UPDATE_EXTRA

    messages = [
        {
            "role": "user",
            "content": (
                f"{instructions}\n\n"
                f"Question: {question}\n\n"
                f"Correct Answer: {ground_truth}\n\n"
                f"Model Response: {predicted}\n\n"
                f"Respond with EXACTLY one word on the first line: 'correct' or 'incorrect'\n"
                f"Then on the next line, a brief explanation."
            ),
        }
    ]

    response = call_llm(messages, model, max_tokens=256)
    first_line = response.strip().split("\n")[0].strip().lower()
    correct = first_line.startswith("correct")

    return {
        "correct": correct,
        "judge_response": response.strip(),
    }


def clear_index():
    """Clear the Memoryport index for a fresh ingestion."""
    if os.path.exists(MEMORYPORT_INDEX):
        shutil.rmtree(MEMORYPORT_INDEX)
        os.makedirs(MEMORYPORT_INDEX)


def main():
    parser = argparse.ArgumentParser(description="LongMemEval Answer Accuracy benchmark")
    parser.add_argument("--questions", type=int, default=50)
    parser.add_argument("--server", type=str, default=SERVER)
    parser.add_argument("--dataset", type=str, default="oracle", choices=["oracle", "s"])
    parser.add_argument("--top-k", type=int, default=50)
    parser.add_argument("--skip-ingest", action="store_true")
    parser.add_argument("--answer-model", type=str, default="claude-sonnet-4-20250514",
                        help="Model for generating answers (e.g., gpt-4o, claude-sonnet-4-20250514)")
    parser.add_argument("--judge-model", type=str, default=None,
                        help="Model for judging (defaults to answer-model)")
    args = parser.parse_args()

    judge_model = args.judge_model or args.answer_model

    # Load dataset
    dataset_file = (
        f"tests/longmemeval/data/longmemeval_{args.dataset}_cleaned.json"
        if args.dataset == "s"
        else "tests/longmemeval/data/longmemeval_oracle.json"
    )
    if not os.path.exists(dataset_file):
        dataset_file = (
            f"data/longmemeval_{args.dataset}_cleaned.json"
            if args.dataset == "s"
            else "data/longmemeval_oracle.json"
        )

    print(f"Loading dataset: {dataset_file}")
    with open(dataset_file) as f:
        all_questions = json.load(f)

    questions = all_questions[: args.questions]
    print(f"Evaluating {len(questions)} questions")
    print(f"  Dataset: {args.dataset}")
    print(f"  Answer model: {args.answer_model}")
    print(f"  Judge model: {judge_model}")
    print(f"  Top-K: {args.top_k}")

    # Phase 1: Ingest
    if not args.skip_ingest:
        print(f"\n[1/3] Ingesting haystacks...")
        total_stored = 0
        for i, q in enumerate(questions):
            stored = ingest_haystack(q, args.server)
            total_stored += stored
            if (i + 1) % 5 == 0:
                print(f"  [{i+1}/{len(questions)}] Ingested {total_stored} turns total")
        print(f"  Total ingested: {total_stored} turns")
    else:
        print(f"\n[1/3] Skipping ingestion (--skip-ingest)")

    # Phase 2: Retrieve
    print(f"\n[2/3] Retrieving context...")
    retrieval_results = []
    for i, q in enumerate(questions):
        result = query_retrieval(q, args.server, args.top_k)
        retrieval_results.append(result)
        if (i + 1) % 10 == 0:
            recalls = [r["session_recall"] for r in retrieval_results if "session_recall" in r]
            avg = statistics.mean(recalls) if recalls else 0
            print(f"  [{i+1}/{len(questions)}] Avg session recall: {avg:.2%}")

    # Phase 3: Answer + Judge
    print(f"\n[3/3] Generating answers and judging accuracy...")
    results = []
    correct_count = 0
    total_evaluated = 0

    for i, (q, retrieval) in enumerate(zip(questions, retrieval_results)):
        if "error" in retrieval:
            results.append({**retrieval, "answer_correct": False, "skipped": True})
            continue

        context = retrieval.get("context", [])
        ground_truth = q["answer"]

        try:
            # Generate answer (with question date for temporal reasoning)
            question_date = q.get("question_date")
            llm_answer = generate_answer(
                q["question"], context, args.answer_model,
                question_date=question_date,
            )

            # Judge answer (with type-specific prompts matching MemoryBench)
            judgment = judge_answer(
                q["question"], ground_truth, llm_answer, judge_model,
                question_type=q.get("question_type"),
            )

            total_evaluated += 1
            if judgment["correct"]:
                correct_count += 1

            result = {
                "qid": retrieval["qid"],
                "question_type": q["question_type"],
                "question": q["question"],
                "ground_truth": ground_truth,
                "llm_answer": llm_answer,
                "answer_correct": judgment["correct"],
                "judge_response": judgment["judge_response"],
                "session_recall": retrieval["session_recall"],
                "latency_ms": retrieval["latency_ms"],
                "num_results": retrieval["num_results"],
            }
            results.append(result)

            status = "+" if judgment["correct"] else "x"
            if (i + 1) % 5 == 0 or i < 5:
                accuracy = correct_count / total_evaluated if total_evaluated else 0
                print(f"  [{i+1}/{len(questions)}] {status} Running accuracy: {accuracy:.2%} ({correct_count}/{total_evaluated})")

        except Exception as e:
            print(f"  [{i+1}/{len(questions)}] ERROR: {e}")
            results.append({
                **retrieval,
                "answer_correct": False,
                "error_answer": str(e),
            })

    # Results
    print(f"\n{'='*70}")
    print(f"LONGMEMEVAL ANSWER ACCURACY RESULTS")
    print(f"{'='*70}")
    print(f"  Questions: {len(results)}")
    print(f"  Dataset: {args.dataset}")
    print(f"  Answer model: {args.answer_model}")
    print(f"  Judge model: {judge_model}")

    evaluated = [r for r in results if not r.get("skipped") and "error_answer" not in r]

    if evaluated:
        correct = sum(1 for r in evaluated if r["answer_correct"])
        accuracy = correct / len(evaluated)
        recalls = [r["session_recall"] for r in evaluated]
        latencies = [r["latency_ms"] for r in evaluated]

        print(f"\n  Answer Accuracy: {accuracy:.2%} ({correct}/{len(evaluated)})")
        print(f"  Session Recall:  {statistics.mean(recalls):.2%}")
        print(f"  Retrieval p50:   {statistics.median(latencies):.0f}ms")

        # By question type
        print(f"\n  By Question Type:")
        print(f"  {'Type':<35s} {'Accuracy':>10s} {'Recall':>10s} {'n':>5s}")
        print(f"  {'-'*35} {'-'*10} {'-'*10} {'-'*5}")

        by_type = {}
        for r in evaluated:
            t = r["question_type"]
            by_type.setdefault(t, []).append(r)

        for t, rs in sorted(by_type.items()):
            type_acc = sum(1 for r in rs if r["answer_correct"]) / len(rs)
            type_recall = statistics.mean([r["session_recall"] for r in rs])
            print(f"  {t:<35s} {type_acc:>9.2%} {type_recall:>9.2%} {len(rs):>5d}")

    errors_answer = [r for r in results if "error_answer" in r]
    errors_retrieval = [r for r in results if "error" in r and "error_answer" not in r]
    if errors_answer or errors_retrieval:
        print(f"\n  Errors: {len(errors_answer)} answer, {len(errors_retrieval)} retrieval")

    # Save results
    output_file = (
        f"tests/longmemeval/results_accuracy_{args.dataset}_{args.questions}q"
        f"_{args.answer_model.replace('/', '_')}.json"
    )
    with open(output_file, "w") as f:
        json.dump(
            {
                "config": {
                    "questions": len(results),
                    "dataset": args.dataset,
                    "answer_model": args.answer_model,
                    "judge_model": judge_model,
                    "top_k": args.top_k,
                    "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                },
                "summary": {
                    "answer_accuracy": correct / len(evaluated) if evaluated else 0,
                    "session_recall": statistics.mean(recalls) if evaluated else 0,
                    "total_evaluated": len(evaluated),
                    "total_correct": correct if evaluated else 0,
                },
                "results": results,
            },
            f,
            indent=2,
        )
    print(f"\n  Results saved to {output_file}")


if __name__ == "__main__":
    main()
