#!/usr/bin/env python3
"""
Generate a query latency vs context space size chart from benchmark results.

Usage:
    python3 tests/scale/plot.py [--input tests/scale/results.json] [--output tests/scale/latency_curve.png]
"""

import argparse
import json
import math
import sys

def format_tokens(n: int) -> str:
    if n >= 1_000_000_000:
        return f"{n / 1_000_000_000:.0f}B"
    if n >= 1_000_000:
        return f"{n / 1_000_000:.0f}M"
    if n >= 1_000:
        return f"{n / 1_000:.0f}K"
    return str(n)

def main():
    parser = argparse.ArgumentParser(description="Plot scale benchmark results")
    parser.add_argument("--input", default="tests/scale/results_50m.json", help="Input JSON")
    parser.add_argument("--output", default="tests/scale/latency_curve.png", help="Output PNG")
    args = parser.parse_args()

    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        import matplotlib.ticker as ticker
    except ImportError:
        print("pip install matplotlib", file=sys.stderr)
        sys.exit(1)

    with open(args.input) as f:
        data = json.load(f)

    if not data:
        print("No data in input file", file=sys.stderr)
        sys.exit(1)

    tokens = [d["token_count"] for d in data]
    p50 = [d["query_latency_p50_ms"] for d in data]
    p95 = [d["query_latency_p95_ms"] for d in data]
    mean = [d["query_latency_mean_ms"] for d in data]

    fig, ax = plt.subplots(figsize=(10, 5))
    fig.patch.set_facecolor("#141210")
    ax.set_facecolor("#1e1c17")

    # Plot lines
    ax.plot(tokens, p50, "o-", color="#f59e0b", linewidth=2, markersize=6, label="p50", zorder=3)
    ax.plot(tokens, p95, "s--", color="#f59e0b", linewidth=1.5, markersize=5, alpha=0.5, label="p95", zorder=3)
    ax.fill_between(tokens, p50, p95, color="#f59e0b", alpha=0.08, zorder=2)

    # Reference line at 500ms
    ax.axhline(y=500, color="#ef4444", linewidth=1, linestyle=":", alpha=0.5, zorder=1)
    ax.text(tokens[0], 520, "500ms target", color="#ef4444", fontsize=9, alpha=0.7)

    # Log scale x-axis
    ax.set_xscale("log")
    ax.set_xlabel("Context Space (tokens)", color="#ede8df", fontsize=11)
    ax.set_ylabel("Query Latency (ms)", color="#ede8df", fontsize=11)
    ax.set_title("Memoryport: Query Latency vs Context Space Size", color="#ede8df", fontsize=13, pad=12)

    # Custom x-axis labels
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_tokens(int(x))))

    # Style
    ax.tick_params(colors="#8a8477")
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    ax.spines["left"].set_color("#332f27")
    ax.spines["bottom"].set_color("#332f27")
    ax.grid(axis="y", color="#332f27", linewidth=0.5, alpha=0.5)
    ax.legend(facecolor="#1e1c17", edgecolor="#332f27", labelcolor="#ede8df", fontsize=10)

    # Annotate each point
    for i, (x, y) in enumerate(zip(tokens, p50)):
        ax.annotate(
            f"{y:.0f}ms",
            (x, y),
            textcoords="offset points",
            xytext=(0, 12),
            ha="center",
            fontsize=8,
            color="#ede8df",
            alpha=0.8,
        )

    plt.tight_layout()
    plt.savefig(args.output, dpi=150, facecolor=fig.get_facecolor())
    print(f"Chart saved to {args.output}")

    # Also print a markdown-friendly summary
    print()
    print("| Context Space | Chunks | p50 | p95 | Index |")
    print("|---|---|---|---|---|")
    for d in data:
        size = f"{d.get('index_size_mb', 0):.0f} MB" if d.get("index_size_mb") else "—"
        print(f"| {format_tokens(d['token_count'])} | {d['chunk_count']:,} | {d['query_latency_p50_ms']:.0f}ms | {d['query_latency_p95_ms']:.0f}ms | {size} |")

if __name__ == "__main__":
    main()
