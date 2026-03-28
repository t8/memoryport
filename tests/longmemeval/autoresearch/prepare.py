#!/usr/bin/env python3
"""
Immutable benchmark harness for autoresearch.

DO NOT MODIFY THIS FILE. The agent modifies experiment.py, not this file.

This script:
  1. Reads experiment config from experiment.py
  2. Builds the server if Rust code changed
  3. Starts the server with experiment config
  4. Ingests the LongMemEval dataset
  5. Runs retrieval + answer accuracy evaluation
  6. Outputs structured results for the agent to parse

Usage:
    python3 tests/longmemeval/autoresearch/prepare.py [--skip-ingest] [--skip-build]
"""

import argparse
import json
import os
import shutil
import signal
import statistics
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime
from pathlib import Path

import requests

# Force unbuffered output so progress is visible in background runs
sys.stdout.reconfigure(line_buffering=True) if hasattr(sys.stdout, 'reconfigure') else None

# ── Paths ───────────────────────────────────────────────────────────────────
ROOT = Path(__file__).resolve().parent.parent.parent.parent
DATASET_DIR = ROOT / "tests" / "longmemeval" / "data"
RESULTS_DIR = ROOT / "tests" / "longmemeval" / "autoresearch"
DATA_DIR = Path.home() / ".memoryport" / "autoresearch_data"
CONFIG_DIR = Path.home() / ".memoryport"
AUTORESEARCH_CONFIG = CONFIG_DIR / "uc_autoresearch.toml"
SERVER_BIN = ROOT / "target" / "debug" / "uc-server"

# ── Constants ───────────────────────────────────────────────────────────────
SERVER_PORT = 8091  # Separate from normal server (8090)
SERVER_URL = f"http://127.0.0.1:{SERVER_PORT}"
SAMPLE_SIZE = 100  # Questions per evaluation run
SAMPLE_SEED = 42   # Reproducible sampling

# ── HTTP Session ────────────────────────────────────────────────────────────
_http = requests.Session()


def load_experiment_config() -> dict:
    """Load the experiment config from experiment.py."""
    config_path = RESULTS_DIR / "experiment.py"
    if not config_path.exists():
        print("ERROR: experiment.py not found. Create it first.")
        sys.exit(1)

    # Import as module
    import importlib.util
    spec = importlib.util.spec_from_file_location("experiment", config_path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.CONFIG


def sample_questions(dataset_path: Path, n: int, seed: int) -> list:
    """Sample a balanced set of questions across all types."""
    import random
    rng = random.Random(seed)

    with open(dataset_path) as f:
        all_questions = json.load(f)

    by_type = {}
    for q in all_questions:
        by_type.setdefault(q["question_type"], []).append(q)

    # Proportional sampling: each type gets its share of n
    types = sorted(by_type.keys())
    total = sum(len(qs) for qs in by_type.values())
    sampled = []

    remaining = n
    for i, t in enumerate(types):
        if i == len(types) - 1:
            count = remaining  # Give remainder to last type
        else:
            count = max(1, round(n * len(by_type[t]) / total))
            count = min(count, remaining, len(by_type[t]))
        remaining -= count
        sampled.extend(rng.sample(by_type[t], count))

    rng.shuffle(sampled)
    return sampled


def write_toml_config(experiment_config: dict):
    """Write a TOML config file for the autoresearch server.

    Reads the base uc.toml config and merges experiment overrides into it,
    replacing section values rather than appending duplicate sections.
    """
    try:
        import tomllib  # Python 3.11+
    except ModuleNotFoundError:
        import tomli as tomllib  # pip install tomli for 3.9/3.10

    base_config_path = CONFIG_DIR / "uc.toml"
    if base_config_path.exists():
        with open(base_config_path, "rb") as f:
            base = tomllib.load(f)
    else:
        base = {}

    # Merge experiment retrieval overrides into base config
    retrieval = experiment_config.get("retrieval", {})
    if "retrieval" not in base:
        base["retrieval"] = {}
    base["retrieval"].update(retrieval)

    # Inject OPENAI_API_KEY into embeddings config if available
    openai_key = os.environ.get("OPENAI_API_KEY")
    if openai_key and "embeddings" in base:
        base["embeddings"]["api_key"] = openai_key

    # Override embeddings model/dimensions if experiment specifies them
    emb_overrides = experiment_config.get("embeddings", {})
    if emb_overrides:
        if "embeddings" not in base:
            base["embeddings"] = {}
        base["embeddings"].update(emb_overrides)
        # Also update index embedding_dimensions to match
        if "dimensions" in emb_overrides:
            if "index" not in base:
                base["index"] = {}
            base["index"]["embedding_dimensions"] = emb_overrides["dimensions"]

    # Override index path to use the isolated autoresearch data directory
    if "index" not in base:
        base["index"] = {}
    base["index"]["path"] = str(DATA_DIR / "index")

    # Serialize back to TOML manually (simple flat structure)
    lines = ["# Autoresearch config (auto-generated, do not edit)"]
    for section, values in base.items():
        if isinstance(values, dict):
            lines.append(f"\n[{section}]")
            for k, v in values.items():
                if isinstance(v, bool):
                    lines.append(f"{k} = {'true' if v else 'false'}")
                elif isinstance(v, str):
                    lines.append(f'{k} = "{v}"')
                elif isinstance(v, float):
                    lines.append(f"{k} = {v}")
                else:
                    lines.append(f"{k} = {v}")
        else:
            # Top-level scalar
            if isinstance(values, bool):
                lines.append(f"{section} = {'true' if values else 'false'}")
            elif isinstance(values, str):
                lines.append(f'{section} = "{values}"')
            else:
                lines.append(f"{section} = {values}")

    with open(AUTORESEARCH_CONFIG, "w") as f:
        f.write("\n".join(lines) + "\n")

    print(f"  Config written to {AUTORESEARCH_CONFIG}")


def build_server() -> bool:
    """Build uc-server. Returns True on success."""
    print("  Building uc-server...")
    result = subprocess.run(
        ["cargo", "build", "-p", "uc-server"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=300,
    )
    if result.returncode != 0:
        print(f"  BUILD FAILED:\n{result.stderr[-1000:]}")
        return False
    print("  Build OK")
    return True


def start_server() -> subprocess.Popen:
    """Start uc-server on the autoresearch port."""
    env = os.environ.copy()
    env["UC_SERVER_LISTEN"] = f"127.0.0.1:{SERVER_PORT}"
    env["UC_SERVER_DATA_DIR"] = str(DATA_DIR)

    proc = subprocess.Popen(
        [str(SERVER_BIN), "--config", str(AUTORESEARCH_CONFIG)],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    # Wait for server to be ready
    for attempt in range(30):
        try:
            r = requests.get(f"{SERVER_URL}/health", timeout=2)
            if r.status_code == 200:
                print(f"  Server ready on port {SERVER_PORT} (pid={proc.pid})")
                return proc
        except Exception:
            pass
        time.sleep(1)

    proc.kill()
    print("  ERROR: Server failed to start within 30s")
    stderr = proc.stderr.read().decode()[-500:]
    print(f"  stderr: {stderr}")
    sys.exit(1)


def stop_server(proc: subprocess.Popen):
    """Gracefully stop the server."""
    proc.send_signal(signal.SIGTERM)
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()
    print("  Server stopped")


def clear_index():
    """Clear the autoresearch data directory (per-user indexes live inside)."""
    if DATA_DIR.exists():
        shutil.rmtree(DATA_DIR)
    DATA_DIR.mkdir(parents=True, exist_ok=True)


def parse_session_date(date_str: str):
    """Parse LongMemEval date to epoch ms."""
    try:
        clean = date_str
        if "(" in clean:
            clean = clean[:clean.index("(")].strip() + " " + clean[clean.index(")") + 1:].strip()
        clean = clean.strip()
        for fmt in ["%Y/%m/%d %H:%M", "%Y/%m/%d", "%Y-%m-%d %H:%M", "%Y-%m-%d"]:
            try:
                dt = datetime.strptime(clean, fmt)
                return int(dt.timestamp() * 1000)
            except ValueError:
                continue
    except Exception:
        pass
    return None


def store_turn(text: str, session_id: str, role: str, timestamp: int = None) -> bool:
    """Store a single turn."""
    try:
        body = {"text": text, "chunk_type": "conversation",
                "session_id": session_id, "role": role}
        if timestamp is not None:
            body["timestamp"] = timestamp
        r = _http.post(f"{SERVER_URL}/v1/store", json=body, timeout=30)
        return r.status_code == 200
    except Exception:
        return False


def ingest_question(question: dict, max_workers: int = 16) -> int:
    """Ingest all haystack sessions for one question. Returns stored count."""
    futures = []
    with ThreadPoolExecutor(max_workers=max_workers) as pool:
        for sid, sdate, sturns in zip(
            question["haystack_session_ids"],
            question["haystack_dates"],
            question["haystack_sessions"],
        ):
            ts = parse_session_date(sdate)
            for idx, turn in enumerate(sturns):
                full_sid = f"{question['question_id']}_{sid}"
                turn_ts = (ts + idx) if ts else None
                futures.append(pool.submit(store_turn, turn["content"],
                                           full_sid, turn["role"], turn_ts))
    return sum(1 for f in as_completed(futures) if f.result())


def _expand_query(query: str) -> list:
    """Use LLM to generate 2-3 alternative phrasings for retrieval."""
    try:
        response = call_llm([{
            "role": "user",
            "content": (
                "Given this search query about a user's conversation history, "
                "generate 3 alternative phrasings that would help find the relevant "
                "conversations. Focus on the key topics and entities, stripping away "
                "temporal/meta language. Return ONLY the alternatives, one per line.\n\n"
                f"Query: {query}"
            ),
        }], "gpt-4o-mini", max_tokens=150)
        return [
            line.strip().lstrip("0123456789.-) ")
            for line in response.strip().split("\n")
            if line.strip() and len(line.strip()) > 5
        ][:3]
    except Exception:
        return []


def _do_retrieve(query: str, top_k: int, reference_time: int = None):
    """Single retrieve call to the server."""
    body = {"query": query, "top_k": top_k}
    if reference_time:
        body["reference_time"] = reference_time
    r = _http.post(f"{SERVER_URL}/v1/retrieve", json=body, timeout=60)
    if r.status_code != 200:
        return []
    return r.json().get("results", [])


def retrieve(question: dict, top_k: int = 50, expand_queries: bool = False) -> dict:
    """Retrieve context for a question, optionally with query expansion."""
    qid = question["question_id"]
    query = question["question"]
    qdate = question.get("question_date")
    ref_ts = parse_session_date(qdate) if qdate else None

    start = time.time()
    try:
        # Primary retrieval
        results = _do_retrieve(query, top_k, ref_ts)

        # Optional: Python-side query expansion (call LLM to rephrase, then merge)
        if expand_queries and results is not None:
            expansions = _expand_query(query)
            seen_ids = {r.get("chunk_id") for r in results}
            for exp_query in expansions:
                exp_results = _do_retrieve(exp_query, top_k // 3, ref_ts)
                for r in exp_results:
                    if r.get("chunk_id") not in seen_ids:
                        seen_ids.add(r.get("chunk_id"))
                        results.append(r)

        latency_ms = (time.time() - start) * 1000
    except Exception as e:
        return {"qid": qid, "error": str(e), "latency_ms": 0}

    # Session recall
    retrieved = set()
    for res in results:
        sid = res.get("session_id", "")
        if "_" in sid:
            retrieved.add(sid.split("_", 1)[1])

    answer_sids = set(question.get("answer_session_ids", []))
    hits = answer_sids & retrieved
    recall = len(hits) / len(answer_sids) if answer_sids else 0.0

    return {
        "qid": qid,
        "question_type": question["question_type"],
        "session_recall": recall,
        "hits": len(hits),
        "answer_sessions": len(answer_sids),
        "latency_ms": latency_ms,
        "num_results": len(results),
        "context": [res.get("content", "") for res in results],
    }


def call_llm(messages: list, model: str, max_tokens: int = 1024) -> str:
    """Call LLM API."""
    if model.startswith("claude"):
        api_key = os.environ.get("ANTHROPIC_API_KEY")
        if not api_key:
            raise ValueError("ANTHROPIC_API_KEY not set")
        r = requests.post(
            "https://api.anthropic.com/v1/messages",
            headers={"x-api-key": api_key, "anthropic-version": "2023-06-01",
                     "content-type": "application/json"},
            json={"model": model, "max_tokens": max_tokens, "messages": messages},
            timeout=120,
        )
        r.raise_for_status()
        return r.json()["content"][0]["text"]
    else:
        api_key = os.environ.get("OPENAI_API_KEY")
        if not api_key:
            raise ValueError("OPENAI_API_KEY not set")
        r = requests.post(
            "https://api.openai.com/v1/chat/completions",
            headers={"Authorization": f"Bearer {api_key}",
                     "Content-Type": "application/json"},
            json={"model": model, "max_tokens": max_tokens, "messages": messages},
            timeout=120,
        )
        r.raise_for_status()
        return r.json()["choices"][0]["message"]["content"]


def generate_answer(question: str, context: list, model: str,
                    question_date: str = None, context_chunks: int = 20,
                    prompt_style: str = "default") -> str:
    """Generate answer from retrieved context."""
    ctx_text = "\n\n---\n\n".join(context[:context_chunks])
    date_line = f"The question was asked on: {question_date}\n\n" if question_date else ""

    if prompt_style == "extract-then-reason":
        # LongMemEval paper's "con" strategy: extract relevant facts first, then reason
        prompt = (
            f"You are answering a question based on your conversation history with the user.\n\n"
            f"{date_line}"
            f"Retrieved conversation history:\n{ctx_text}\n\n"
            f"Question: {question}\n\n"
            f"Follow these steps:\n"
            f"1. EXTRACT: List all facts from the conversation history that are relevant "
            f"to answering this question. Include dates, names, and specific details.\n"
            f"2. REASON: Using only the extracted facts, reason step by step to arrive "
            f"at the answer. For temporal questions, explicitly calculate time differences. "
            f"For questions about order, explicitly compare dates.\n"
            f"3. ANSWER: State your final answer concisely.\n"
        )
    else:
        prompt = (
            f"You are answering a question based on your conversation history with "
            f"the user. Use the retrieved conversation excerpts below to answer.\n\n"
            f"{date_line}"
            f"Retrieved conversation history:\n{ctx_text}\n\n"
            f"Question: {question}\n\n"
            f"Answer the question concisely based on the conversation history above. "
            f"Extract all relevant information and reason step by step if needed. "
            f"Pay attention to dates and temporal ordering of events."
        )

    return call_llm([{"role": "user", "content": prompt}], model, max_tokens=768)


# Type-specific judge prompts (matching MemoryBench methodology)
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


def judge_answer(question: str, ground_truth: str, predicted: str,
                 model: str, question_type: str = None) -> dict:
    """LLM-as-judge evaluation."""
    instructions = JUDGE_BASE
    if question_type == "temporal-reasoning":
        instructions += JUDGE_TEMPORAL_EXTRA
    elif question_type == "knowledge-update":
        instructions += JUDGE_KNOWLEDGE_UPDATE_EXTRA

    response = call_llm([{
        "role": "user",
        "content": (
            f"{instructions}\n\n"
            f"Question: {question}\n\nCorrect Answer: {ground_truth}\n\n"
            f"Model Response: {predicted}\n\n"
            f"Respond with EXACTLY one word on the first line: 'correct' or 'incorrect'\n"
            f"Then on the next line, a brief explanation."
        ),
    }], model, max_tokens=256)

    first_line = response.strip().split("\n")[0].strip().lower()
    return {"correct": first_line.startswith("correct"), "judge_response": response.strip()}


def run_evaluation(questions: list, experiment_config: dict) -> dict:
    """Run full retrieval + answer accuracy evaluation."""
    top_k = experiment_config.get("retrieval", {}).get("similarity_top_k", 50)
    answer_model = experiment_config.get("answer_model", "gpt-4o-mini")
    judge_model = experiment_config.get("judge_model", "gpt-4o-mini")

    # Phase 1: Retrieve
    print("\n  [2/3] Retrieving context...")
    retrievals = []
    for i, q in enumerate(questions):
        expand = experiment_config.get("expand_queries", False)
        r = retrieve(q, top_k=top_k, expand_queries=expand)
        retrievals.append(r)
        if (i + 1) % 20 == 0:
            recalls = [x["session_recall"] for x in retrievals if "session_recall" in x]
            print(f"    [{i+1}/{len(questions)}] Avg recall: {statistics.mean(recalls):.2%}")

    # Phase 2: Answer + Judge
    print("\n  [3/3] Generating answers and judging...")
    results = []
    correct = 0
    evaluated = 0

    for i, (q, ret) in enumerate(zip(questions, retrievals)):
        if "error" in ret:
            results.append({**ret, "answer_correct": False, "skipped": True})
            continue

        try:
            context_chunks = experiment_config.get("context_chunks", 20)
            prompt_style = experiment_config.get("prompt_style", "default")
            answer = generate_answer(
                q["question"], ret.get("context", []), answer_model,
                question_date=q.get("question_date"),
                context_chunks=context_chunks,
                prompt_style=prompt_style,
            )
            judgment = judge_answer(
                q["question"], q["answer"], answer, judge_model,
                question_type=q.get("question_type"),
            )
            evaluated += 1
            if judgment["correct"]:
                correct += 1

            results.append({
                "qid": ret["qid"],
                "question_type": q["question_type"],
                "question": q["question"],
                "ground_truth": q["answer"],
                "llm_answer": answer,
                "answer_correct": judgment["correct"],
                "judge_response": judgment["judge_response"],
                "session_recall": ret["session_recall"],
                "latency_ms": ret["latency_ms"],
            })

            if (i + 1) % 10 == 0:
                acc = correct / evaluated if evaluated else 0
                print(f"    [{i+1}/{len(questions)}] Accuracy: {acc:.2%} ({correct}/{evaluated})")

        except Exception as e:
            print(f"    [{i+1}/{len(questions)}] ERROR: {e}")
            results.append({**ret, "answer_correct": False, "error_answer": str(e)})

    # Aggregate
    valid = [r for r in results if not r.get("skipped") and "error_answer" not in r]
    by_type = {}
    for r in valid:
        by_type.setdefault(r["question_type"], []).append(r)

    type_accuracy = {}
    for t, rs in sorted(by_type.items()):
        type_accuracy[t] = sum(1 for r in rs if r["answer_correct"]) / len(rs) if rs else 0

    latencies = [r["latency_ms"] for r in valid]
    recalls = [r["session_recall"] for r in valid]

    summary = {
        "answer_accuracy": correct / evaluated if evaluated else 0,
        "session_recall": statistics.mean(recalls) if recalls else 0,
        "latency_p50": statistics.median(latencies) if latencies else 0,
        "latency_p95": sorted(latencies)[int(len(latencies) * 0.95)] if latencies else 0,
        "evaluated": evaluated,
        "correct": correct,
        "type_accuracy": type_accuracy,
    }

    return {"summary": summary, "results": results}


def main():
    parser = argparse.ArgumentParser(description="Autoresearch benchmark harness")
    parser.add_argument("--skip-ingest", action="store_true",
                        help="Skip ingestion (reuse existing index)")
    parser.add_argument("--skip-build", action="store_true",
                        help="Skip cargo build")
    parser.add_argument("--dataset", default="s", choices=["oracle", "s"],
                        help="Dataset variant (default: s)")
    parser.add_argument("--questions", type=int, default=SAMPLE_SIZE,
                        help=f"Number of questions (default: {SAMPLE_SIZE})")
    args = parser.parse_args()

    experiment_config = load_experiment_config()

    # Verify required env vars
    if not os.environ.get("OPENAI_API_KEY"):
        print("ERROR: OPENAI_API_KEY environment variable not set.")
        print("  export OPENAI_API_KEY='sk-...'")
        sys.exit(1)

    print(f"{'='*70}")
    print(f"AUTORESEARCH BENCHMARK RUN")
    print(f"{'='*70}")
    print(f"  Dataset: longmemeval_{args.dataset}")
    print(f"  Questions: {args.questions}")
    print(f"  Config: {json.dumps(experiment_config.get('retrieval', {}), indent=2)}")

    # Build
    if not args.skip_build:
        if not build_server():
            sys.exit(1)

    # Write config
    write_toml_config(experiment_config)

    # Start server
    proc = start_server()

    try:
        # Sample questions
        dataset_name = f"longmemeval_{args.dataset}_cleaned.json" if args.dataset == "s" else "longmemeval_oracle.json"
        dataset_path = DATASET_DIR / dataset_name
        questions = sample_questions(dataset_path, args.questions, SAMPLE_SEED)
        print(f"  Sampled {len(questions)} questions")

        types = {}
        for q in questions:
            types[q["question_type"]] = types.get(q["question_type"], 0) + 1
        for t, c in sorted(types.items()):
            print(f"    {t}: {c}")

        # Ingest
        if not args.skip_ingest:
            clear_index()
            print("\n  [1/3] Ingesting haystacks...")
            total = 0
            for i, q in enumerate(questions):
                stored = ingest_question(q)
                total += stored
                if (i + 1) % 10 == 0:
                    print(f"    [{i+1}/{len(questions)}] Ingested {total} turns")
            print(f"    Total: {total} turns")
            # Wait for indexing to settle
            time.sleep(2)
        else:
            print("\n  [1/3] Skipping ingestion")

        # Evaluate
        eval_result = run_evaluation(questions, experiment_config)
        summary = eval_result["summary"]

        # Print results
        print(f"\n{'='*70}")
        print(f"RESULTS")
        print(f"{'='*70}")
        print(f"  Answer Accuracy: {summary['answer_accuracy']:.2%} ({summary['correct']}/{summary['evaluated']})")
        print(f"  Session Recall:  {summary['session_recall']:.2%}")
        print(f"  Latency p50:     {summary['latency_p50']:.0f}ms")
        print(f"  Latency p95:     {summary['latency_p95']:.0f}ms")
        print(f"\n  By Type:")
        for t, acc in sorted(summary["type_accuracy"].items()):
            print(f"    {t:<35s} {acc:.2%}")

        # Output parseable line for agent
        print(f"\n{'='*70}")
        print(f"PARSEABLE:")
        type_str = " ".join(f"{t}={acc:.4f}" for t, acc in sorted(summary["type_accuracy"].items()))
        print(f"overall_accuracy={summary['answer_accuracy']:.4f} "
              f"session_recall={summary['session_recall']:.4f} "
              f"latency_p50={summary['latency_p50']:.0f} "
              f"latency_p95={summary['latency_p95']:.0f} "
              f"{type_str}")

        # Save full results
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        commit_hash = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            capture_output=True, text=True, cwd=ROOT,
        ).stdout.strip()

        output_path = RESULTS_DIR / f"run_{timestamp}_{commit_hash}.json"
        with open(output_path, "w") as f:
            json.dump({
                "config": experiment_config,
                "summary": summary,
                "results": eval_result["results"],
                "timestamp": timestamp,
                "commit": commit_hash,
            }, f, indent=2)
        print(f"\n  Full results: {output_path}")

    finally:
        stop_server(proc)


if __name__ == "__main__":
    main()
