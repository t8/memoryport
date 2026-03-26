#!/usr/bin/env python3
"""
Run LongMemEval with a balanced mix of question types.
Samples evenly from each category.
"""

import json
import random
import subprocess
import sys
import os

random.seed(42)

with open("tests/longmemeval/data/longmemeval_oracle.json") as f:
    data = json.load(f)

# Group by type
by_type = {}
for q in data:
    by_type.setdefault(q["question_type"], []).append(q)

print("Dataset distribution:")
for t, qs in sorted(by_type.items()):
    print(f"  {t}: {len(qs)}")

# Sample 8 from each type (or all if fewer)
n_per_type = 8
sampled = []
for t, qs in sorted(by_type.items()):
    sampled.extend(random.sample(qs, min(n_per_type, len(qs))))

random.shuffle(sampled)
print(f"\nSampled {len(sampled)} questions")

# Write temp file
with open("tests/longmemeval/data/sampled_mixed.json", "w") as f:
    json.dump(sampled, f)

print("Saved to tests/longmemeval/data/sampled_mixed.json")
print(f"Run: python3 tests/longmemeval/run_benchmark.py --questions {len(sampled)} --dataset oracle")
