#!/usr/bin/env python3
"""
Latency benchmark: measures proxy overhead for memoryport in three modes.

Compares:
  1. Direct → mock  (baseline, no proxy)
  2. Proxy single-turn  (context injection, agentic disabled via header)
  3. Proxy multi-turn  (agentic loop enabled, mock returns 1 tool round)

Prerequisites:
  - Mock upstream running: python3 tests/latency/mock_upstream.py --port 8199
  - Proxy running pointed at mock:
      ./target/debug/uc-proxy --config ~/.memoryport/uc.toml --listen 127.0.0.1:9191

    To point the proxy at the mock, set upstream in uc.toml:
      [proxy]
      listen = "127.0.0.1:9191"
      upstream = "http://127.0.0.1:8199"

Usage:
    python3 tests/latency/benchmark.py \\
        --proxy http://127.0.0.1:9191 \\
        --mock http://127.0.0.1:8199 \\
        --iterations 50

    Options:
        --proxy       Proxy URL (default: http://127.0.0.1:9191)
        --mock        Mock upstream URL (default: http://127.0.0.1:8199)
        --iterations  Number of requests per mode (default: 50)
        --warmup      Warmup requests before measuring (default: 3)
"""

import argparse
import json
import statistics
import sys
import time
import uuid
import requests


def make_anthropic_request(query: str = "What did we discuss about authentication?") -> dict:
    """Build a minimal Anthropic Messages API request."""
    return {
        "model": "mock-model",
        "max_tokens": 1024,
        "messages": [
            {"role": "user", "content": query},
        ],
    }


def percentile(data: list[float], p: float) -> float:
    """Get the p-th percentile from sorted data."""
    if not data:
        return 0.0
    k = (len(data) - 1) * (p / 100.0)
    f = int(k)
    c = f + 1
    if c >= len(data):
        return data[-1]
    return data[f] + (k - f) * (data[c] - data[f])


def run_mode(
    label: str,
    url: str,
    headers: dict,
    iterations: int,
    warmup: int,
) -> list[float]:
    """Run a single benchmark mode and return latencies in ms."""
    latencies = []

    for i in range(warmup + iterations):
        # Use a unique query each time to avoid mock caching artifacts
        query = f"What did we discuss about authentication? (run {uuid.uuid4().hex[:8]})"
        payload = make_anthropic_request(query)

        start = time.perf_counter()
        try:
            resp = requests.post(
                url,
                json=payload,
                headers=headers,
                timeout=30,
            )
            elapsed_ms = (time.perf_counter() - start) * 1000

            if resp.status_code != 200:
                print(f"  [{label}] WARNING: status {resp.status_code} on iteration {i}")
                continue

            if i >= warmup:
                latencies.append(elapsed_ms)

        except requests.RequestException as e:
            print(f"  [{label}] ERROR: {e}")
            continue

    return latencies


def print_results(results: dict[str, list[float]]):
    """Print a formatted results table."""
    baseline = results.get("direct")

    print()
    print("=" * 78)
    print("LATENCY BENCHMARK RESULTS")
    print("=" * 78)
    print()
    print(f"{'Mode':<20} {'p50':>8} {'p95':>8} {'p99':>8} {'mean':>8} {'stddev':>8} {'overhead':>10}")
    print("-" * 78)

    for label, latencies in results.items():
        if not latencies:
            print(f"{label:<20} {'(no data)':>8}")
            continue

        s = sorted(latencies)
        p50 = percentile(s, 50)
        p95 = percentile(s, 95)
        p99 = percentile(s, 99)
        mean = statistics.mean(latencies)
        sd = statistics.stdev(latencies) if len(latencies) > 1 else 0.0

        overhead = ""
        if baseline and label != "direct" and baseline:
            base_mean = statistics.mean(baseline)
            delta = mean - base_mean
            overhead = f"+{delta:.1f}ms"

        print(
            f"{label:<20} {p50:>7.1f}ms {p95:>7.1f}ms {p99:>7.1f}ms "
            f"{mean:>7.1f}ms {sd:>7.1f}ms {overhead:>10}"
        )

    print()
    print(f"Iterations per mode: {len(next(iter(results.values()), []))}")

    # Save results to JSON
    results_data = {}
    for label, latencies in results.items():
        if latencies:
            s = sorted(latencies)
            results_data[label] = {
                "p50_ms": round(percentile(s, 50), 2),
                "p95_ms": round(percentile(s, 95), 2),
                "p99_ms": round(percentile(s, 99), 2),
                "mean_ms": round(statistics.mean(latencies), 2),
                "stddev_ms": round(statistics.stdev(latencies) if len(latencies) > 1 else 0, 2),
                "samples": len(latencies),
            }

    if results_data and "direct" in results_data:
        base_mean = results_data["direct"]["mean_ms"]
        for label in results_data:
            if label != "direct":
                results_data[label]["overhead_ms"] = round(
                    results_data[label]["mean_ms"] - base_mean, 2
                )

    with open("tests/latency/results.json", "w") as f:
        json.dump(results_data, f, indent=2)
    print(f"Results saved to tests/latency/results.json")


def check_server(url: str, name: str) -> bool:
    """Check if a server is reachable."""
    try:
        resp = requests.get(url.rstrip("/") + "/health", timeout=5)
        return True
    except requests.RequestException:
        try:
            # Try root path as fallback
            resp = requests.get(url.rstrip("/") + "/", timeout=5)
            return True
        except requests.RequestException:
            return False


def main():
    parser = argparse.ArgumentParser(
        description="Latency benchmark: measures proxy overhead for memoryport"
    )
    parser.add_argument(
        "--proxy", type=str, default="http://127.0.0.1:9191",
        help="Proxy URL (default: http://127.0.0.1:9191)",
    )
    parser.add_argument(
        "--mock", type=str, default="http://127.0.0.1:8199",
        help="Mock upstream URL (default: http://127.0.0.1:8199)",
    )
    parser.add_argument(
        "--iterations", type=int, default=50,
        help="Requests per mode (default: 50)",
    )
    parser.add_argument(
        "--warmup", type=int, default=3,
        help="Warmup requests before measuring (default: 3)",
    )
    args = parser.parse_args()

    # Connectivity checks
    print("Checking servers...")
    if not check_server(args.mock, "mock upstream"):
        print(f"ERROR: Mock upstream not reachable at {args.mock}")
        print(f"Start it: python3 tests/latency/mock_upstream.py --port 8199")
        sys.exit(1)

    proxy_available = check_server(args.proxy, "proxy")
    if not proxy_available:
        print(f"WARNING: Proxy not reachable at {args.proxy}")
        print(f"Only running baseline (direct → mock). Start the proxy for full benchmark.")
        print()

    print(f"Mock upstream: {args.mock}")
    if proxy_available:
        print(f"Proxy:         {args.proxy}")
    print(f"Iterations:    {args.iterations} (+ {args.warmup} warmup)")
    print()

    results = {}

    # Mode 1: Direct → mock (baseline)
    print("Running: direct → mock (baseline)...")
    results["direct"] = run_mode(
        label="direct",
        url=f"{args.mock}/v1/messages",
        headers={"Content-Type": "application/json"},
        iterations=args.iterations,
        warmup=args.warmup,
    )
    print(f"  done ({len(results['direct'])} samples)")

    if proxy_available:
        # Mode 2: Proxy single-turn (agentic disabled)
        print("Running: proxy single-turn (agentic disabled)...")
        results["single-turn"] = run_mode(
            label="single-turn",
            url=f"{args.proxy}/v1/messages",
            headers={
                "Content-Type": "application/json",
                "x-api-key": "test-key",
                "x-memoryport-agentic": "false",
            },
            iterations=args.iterations,
            warmup=args.warmup,
        )
        print(f"  done ({len(results['single-turn'])} samples)")

        # Mode 3: Proxy multi-turn (agentic enabled)
        print("Running: proxy multi-turn (agentic enabled)...")
        results["multi-turn"] = run_mode(
            label="multi-turn",
            url=f"{args.proxy}/v1/messages",
            headers={
                "Content-Type": "application/json",
                "x-api-key": "test-key",
            },
            iterations=args.iterations,
            warmup=args.warmup,
        )
        print(f"  done ({len(results['multi-turn'])} samples)")

    print_results(results)


if __name__ == "__main__":
    main()
