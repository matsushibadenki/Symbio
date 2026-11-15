#!/usr/bin/env python3
"""Evaluate and calibrate Symbio's actual beam decoder on MathWriting InkML."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import math
from pathlib import Path
import re
import random
import subprocess
import tarfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
NS = "{http://www.w3.org/2003/InkML}"
TOKEN = re.compile(r"\\[a-zA-Z]+|\\.|[^\s]")


def formula(latex):
    # Ignore presentation spacing and redundant braces around a single script atom.
    tokens = [t for t in TOKEN.findall(latex)
              if t not in {r"\left", r"\right", r"\,", r"\!", r"\;", r"\:", r"\ ", "~"}]
    normalized = []
    i = 0
    while i < len(tokens):
        if (i + 3 < len(tokens) and tokens[i] in {"_", "^"} and
                tokens[i + 1] == "{" and tokens[i + 2] not in {"{", "}"} and tokens[i + 3] == "}"):
            normalized.extend((tokens[i], tokens[i + 2]))
            i += 4
        else:
            normalized.append(tokens[i])
            i += 1
    return normalized


def load(archive, max_tokens=None, limit=None, excluded=frozenset()):
    splits = {"valid": [], "test": []}
    with tarfile.open(archive, "r|gz") as tar:
        for member in tar:
            path = Path(member.name)
            if not member.isfile() or path.suffix != ".inkml" or path.parent.name not in splits:
                continue
            root = ET.fromstring(tar.extractfile(member).read())
            labels = {a.get("type"): a.text for a in root.findall(NS + "annotation")}
            split = path.parent.name
            if labels["inkCreationMethod"] != "human" or labels["splitTagOriginal"] != split:
                raise ValueError(f"Unexpected sample provenance: {path}")
            if path.stem in excluded or (max_tokens is not None and
                                          len(formula(labels["normalizedLabel"])) > max_tokens):
                continue
            strokes = [[tuple(map(float, point.split()[:2])) for point in trace.text.split(",")]
                       for trace in root.findall(NS + "trace")]
            points = [point for stroke in strokes for point in stroke]
            left, top = map(min, zip(*points))
            right, bottom = map(max, zip(*points))
            # Match the application's logical pad while keeping the aspect ratio.
            scale = min(980 / max(1, right - left), 300 / max(1, bottom - top))
            ink = {"strokes": []}
            for stroke in strokes:
                xy = [{"x": 10 + (x - left) * scale, "y": 10 + (y - top) * scale}
                      for x, y in stroke]
                if len(xy) == 1:
                    xy.append({"x": xy[0]["x"] + 0.1, "y": xy[0]["y"]})
                ink["strokes"].append(xy)
            splits[split].append((path.stem, labels["normalizedLabel"], ink))
    if not all(splits.values()) or {r[0] for r in splits["valid"]} & {r[0] for r in splits["test"]}:
        raise ValueError("Need nonempty, disjoint validation and test samples")
    for split, rows in splits.items():
        rows.sort(key=lambda row: hashlib.sha256(row[0].encode()).digest())
        if limit is not None:
            splits[split] = rows[:limit]
    return splits


def features(result):
    # Geometric mean token probabilities. Each is a score proxy, not an event probability.
    candidates = result.get("candidates", [])[:3]
    scores = [math.exp(c["log_score"] / (c["tokens"] + 1)) if c["complete"] else 0
              for c in candidates]
    return (scores[0] if scores else 0, max(scores, default=0))


def sigmoid(x):
    return 1 / (1 + math.exp(-max(-60, min(60, x))))


def fit(xs, ys):
    # Two-parameter monotone logistic calibration, regularized for small samples.
    bias, weight = 0., 0.
    for _ in range(3000):
        residuals = [sigmoid(bias + weight * x) - y for x, y in zip(xs, ys)]
        bias -= 0.25 * sum(residuals) / len(xs)
        weight = max(0., weight - 0.25 * (sum(r*x for r, x in zip(residuals, xs)) / len(xs)
                                         + 0.005 * weight))
    return bias, weight


def metrics(predictions, hits):
    n = len(hits)
    bins = [[] for _ in range(5)]
    for probability, hit in zip(predictions, hits):
        bins[min(4, int(probability * 5))].append((probability, hit))
    reliability = [
        {"count": len(group), "mean_probability": sum(p for p, _ in group) / len(group),
         "hit_rate": sum(y for _, y in group) / len(group)}
        for group in bins if group
    ]
    return {"hits": sum(hits), "accuracy": sum(hits) / n,
            "brier": sum((p - y)**2 for p, y in zip(predictions, hits)) / n,
            "ece_5_bins": sum(g["count"] * abs(g["mean_probability"] - g["hit_rate"])
                              for g in reliability) / n, "reliability": reliability}


def candidate_score(candidate):
    if not candidate["complete"]:
        return 0.
    return math.exp(candidate["log_score"] / (candidate["tokens"] + 1))


def brier_delta_interval(predictions, hits, baseline):
    differences = [(p - y)**2 - (baseline - y)**2 for p, y in zip(predictions, hits)]
    rng = random.Random(20261115)
    n = len(differences)
    means = sorted(sum(rng.choices(differences, k=n)) / n for _ in range(2000))
    return [means[50], means[1949]]


def candidate_calibration(rows, results, cut):
    report = {}
    for rank in range(3):
        samples = {"valid": [], "test": []}
        for i, ((_, label, _), result) in enumerate(zip(rows, results)):
            candidates = result.get("candidates", [])
            if len(candidates) <= rank:
                continue
            candidate = candidates[rank]
            hit = int(candidate["complete"] and formula(candidate["latex"]) == formula(label))
            samples["valid" if i < cut else "test"].append((candidate_score(candidate), hit))
        if not all(samples.values()):
            continue
        train_scores, train_hits = zip(*samples["valid"])
        test_scores, test_hits = zip(*samples["test"])
        bias, weight = fit(train_scores, train_hits)
        baseline = sum(train_hits) / len(train_hits)
        probabilities = [sigmoid(bias + weight * score) for score in test_scores]
        report[str(rank + 1)] = {
            "validation_candidates": len(train_hits),
            "test_candidates": len(test_hits),
            "bias": bias, "weight": weight,
            "test_constant_baseline": metrics([baseline] * len(test_hits), test_hits),
            "test_calibrated": metrics(probabilities, test_hits),
            "probability_range": [min(probabilities), max(probabilities)],
            "brier_delta_bootstrap_95": brier_delta_interval(probabilities, test_hits, baseline),
        }
    return report


def sha256(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest() if hasattr(hashlib, "file_digest") else stream_digest(source)


def stream_digest(source):
    digest = hashlib.sha256()
    for block in iter(lambda: source.read(1024 * 1024), b""):
        digest.update(block)
    return digest.hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("models", type=Path)
    parser.add_argument("runtime", type=Path)
    parser.add_argument("report", type=Path)
    parser.add_argument("--max-tokens", type=int, help="Keep labels up to this normalized token count")
    parser.add_argument("--limit-per-split", type=int, help="Deterministic hash sample from each split")
    parser.add_argument("--exclude-excerpt", type=Path, help="Exclude earlier validation/test sample IDs")
    parser.add_argument("--jobs", type=int, default=1, choices=range(1, 9),
                        help="Independent inference workers")
    args = parser.parse_args()
    if any(value is not None and value < 1 for value in (args.max_tokens, args.limit_per_split)):
        parser.error("limits must be positive")
    excluded = {row[0] for rows in load(args.exclude_excerpt).values() for row in rows} if args.exclude_excerpt else set()
    splits = load(args.archive, args.max_tokens, args.limit_per_split, excluded)
    print({split: len(samples) for split, samples in splits.items()}, flush=True)
    subprocess.run(["cargo", "build", "--locked", "--offline", "-p", "symbio-handwriting",
                    "--example", "benchmark"], cwd=ROOT, check=True)
    rows = [row for split in ("valid", "test") for row in splits[split]]
    def infer(batch):
        result = subprocess.run([ROOT / "target/debug/examples/benchmark", args.models, args.runtime],
                                input="".join(json.dumps(ink) + "\n" for _, _, ink in batch),
                                text=True, stdout=subprocess.PIPE, check=True)
        predictions = [json.loads(line) for line in result.stdout.splitlines()]
        if len(predictions) != len(batch):
            raise ValueError("Recognizer output row count differs from input")
        return predictions

    size = math.ceil(len(rows) / args.jobs)
    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        results = [result for batch in executor.map(infer, [
            rows[start:start + size] for start in range(0, len(rows), size)
        ]) for result in batch]
    hits = [[], []]
    for (_, label, _), prediction in zip(rows, results):
        expected = formula(label)
        matches = [c["complete"] and formula(c["latex"]) == expected
                   for c in prediction.get("candidates", [])[:3]]
        hits[0].append(int(bool(matches and matches[0])))
        hits[1].append(int(any(matches)))
    inputs = list(zip(*(features(result) for result in results)))
    cut = len(splits["valid"])
    report = {
        "method": "mathwriting-monotone-logistic-v2",
        "archive_sha256": sha256(args.archive),
        "model_sha256": {name: hashlib.sha256((args.models / name).read_bytes()).hexdigest()
                         for name in ("encoder_int8.onnx", "decoder_int8.onnx", "vocab.json")},
        "runtime_sha256": hashlib.sha256(args.runtime.read_bytes()).hexdigest(),
        "samples": {split: len(samples) for split, samples in splits.items()},
        "selection": {"max_tokens": args.max_tokens, "limit_per_split": args.limit_per_split,
                      "excluded_excerpt_sha256": sha256(args.exclude_excerpt) if args.exclude_excerpt else None},
        "selected_ids_sha256": {
            split: hashlib.sha256("\n".join(row[0] for row in samples).encode()).hexdigest()
            for split, samples in splits.items()
        },
        "candidate_calibration": candidate_calibration(rows, results, cut),
        "inference_errors": sum("error" in r for r in results),
    }
    for name, scores, ys in zip(("top1", "top3"), inputs, hits):
        bias, weight = fit(scores[:cut], ys[:cut])
        after = [sigmoid(bias + weight * score) for score in scores[cut:]]
        report[name] = {"bias": bias, "weight": weight, "validation_hits": sum(ys[:cut]),
                        "test_raw": metrics(scores[cut:], ys[cut:]),
                        "test_constant_baseline": metrics([sum(ys[:cut]) / cut] * len(after), ys[cut:]),
                        "test_calibrated": metrics(after, ys[cut:])}
    with args.report.open("x") as output:
        json.dump(report, output, indent=2)
        output.write("\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
