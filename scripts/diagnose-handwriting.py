#!/usr/bin/env python3
"""Classify token edits in the closest beam candidate for each Top-3 miss."""
import argparse
from collections import Counter
import json
from pathlib import Path
import runpy

FORMULA = runpy.run_path(str(Path(__file__).with_name("benchmark-handwriting.py")))["formula"]
BARS = {"|", r"\mid", r"\vert", r"\lvert", r"\rvert", r"\|"}


def tagged(latex):
    tokens = FORMULA(latex)
    result = []
    groups = []
    pending = False
    for token in tokens:
        in_script = pending or bool(groups and groups[-1])
        if token == "{":
            groups.append(in_script)
        elif token == "}" and groups:
            groups.pop()
        category = ("bar" if token in BARS else
                    "script" if in_script or token in {"_", "^"} else "surround")
        result.append((token, category))
        pending = token in {"_", "^"}
    return result


def edits(reference, predicted):
    """Unit-cost token alignment; ties prefer substitutions, then deletions."""
    n, m = len(reference), len(predicted)
    cost = [[i + j if i == 0 or j == 0 else 0 for j in range(m+1)] for i in range(n+1)]
    for i in range(1, n+1):
        for j in range(1, m+1):
            cost[i][j] = min(cost[i-1][j] + 1, cost[i][j-1] + 1,
                             cost[i-1][j-1] + (reference[i-1][0] != predicted[j-1][0]))
    i, j = n, m
    differences = []
    while i or j:
        left = reference[i-1] if i else None
        right = predicted[j-1] if j else None
        if i and j and cost[i][j] == cost[i-1][j-1] + (left[0] != right[0]):
            i, j = i-1, j-1
            if left[0] == right[0]:
                continue
        elif i and cost[i][j] == cost[i-1][j] + 1:
            i -= 1
            right = None
        else:
            j -= 1
            left = None
        kinds = {item[1] for item in (left, right) if item is not None}
        category = next(k for k in ("bar", "script", "surround") if k in kinds)
        differences.append((category, left[0] if left else "", right[0] if right else ""))
    return differences


def diagnose(labels, predictions):
    summary = {"samples": len(labels), "top1": 0, "top3": 0, "inference_errors": 0}
    counts, expressions, substitutions = Counter(), Counter(), Counter()
    for label, prediction in zip(labels, predictions, strict=True):
        reference = tagged(label)
        candidates = prediction.get("candidates", [])[:3]
        summary["inference_errors"] += "error" in prediction
        scored = [(edits(reference, tagged(c["latex"])), c["complete"]) for c in candidates]
        hits = [not changes and complete for changes, complete in scored]
        summary["top1"] += bool(hits and hits[0])
        summary["top3"] += any(hits)
        if any(hits):
            continue
        available = [changes for changes, complete in scored if complete] or [changes for changes, _ in scored]
        closest = min(available, key=len) if available else edits(reference, [])
        counts.update(category for category, _, _ in closest)
        expressions.update({category for category, _, _ in closest})
        substitutions.update(closest)
    return {**summary, "edits": dict(counts), "affected_misses": dict(expressions),
            "common_edits": substitutions.most_common(15)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("labels", type=Path, help="JSONL objects containing label and optional cohort")
    parser.add_argument("predictions", type=Path, nargs="?", help="Separate Rust prediction JSONL")
    parser.add_argument("--order", choices=("before", "after"), default="after",
                        help="Candidate order for benchmark --diagnostic-rows input")
    args = parser.parse_args()
    labels = [json.loads(line) for line in args.labels.read_text().splitlines()]
    predictions = (
        [json.loads(line) for line in args.predictions.read_text().splitlines()]
        if args.predictions else [row.get(args.order, row) for row in labels]
    )
    pairs = list(zip(labels, predictions, strict=True))
    report = {}
    for cohort in sorted({row.get("cohort", "all") for row in labels}):
        group = [(row["label"], prediction) for row, prediction in pairs
                 if row.get("cohort", "all") == cohort]
        report[cohort] = diagnose(*zip(*group))
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
