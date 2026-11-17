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


BARE_BARS = {"|", r"\mid", r"\vert"}


def indexed_divisibility(latex):
    tokens = formula(latex)
    return (
        sum(token in BARE_BARS for token in tokens) == 1
        and re.search(r"\\(?:sum|prod)_\{[dp](?:\||\\mid|\\vert)n\}", "".join(tokens))
        is not None
    )


def bar_cohort(latex, expanded_arithmetic=False):
    tokens = formula(latex)
    bars = [i for i, token in enumerate(tokens) if token in BARE_BARS]
    if len(bars) == 1 and not any(token in {r"\lvert", r"\rvert", r"\|"} for token in tokens):
        i = bars[0]
        if 0 < i < len(tokens) - 1:
            prefix = "".join(tokens[:i])
            if re.search(r"(?:P|Pr|\\Pr|\\mathbb\{P\})[([][^)\]]*$", prefix):
                return "conditional_probability"
            suffix = "".join(tokens[i + 1:])
            if (re.fullmatch(r"[0-9]+", prefix) and re.fullmatch(r"[0-9]+", suffix)):
                return "integer_divisibility"
            if (tokens[i] == r"\mid" and re.fullmatch(r"[a-zA-Z0-9]+", prefix)
                    and re.fullmatch(r"[a-zA-Z0-9]+", suffix)):
                return "explicit_divisibility_notation"
            return "other_single_bar"
    if (
        ("1" in tokens or expanded_arithmetic)
        and any(token in {"+", "-"} for token in tokens)
        and len(tokens) <= 16
        and all(re.fullmatch(r"[a-zA-Z0-9+\-]", token) for token in tokens)
    ):
        return "flat_arithmetic"
    return None


def conditional_audit_cohort(latex):
    # Lowercase conditional density is independent of the earlier P/Pr sets.
    if re.search(r"(?<![A-Za-z\\])p\([^()]*\|[^()]*\)", latex):
        return "conditional_density"
    if re.fullmatch(r"[A-Za-z]+/[A-Za-z]+", latex):
        return "symbol_ratio"
    if re.search(r"(?<![A-Za-z\\])(?:Pr?|\\Pr)\([^()|]*/[^()|]*\)", latex):
        return "slash_in_probability_shaped_call"
    return None


def load(archive, max_tokens=None, limit=None, excluded=frozenset(), target_bars=False, bar_split="test", only_indexed_divisibility=False, expanded_arithmetic=False,
         only_indexed_products=False, conditional_audit=False):
    splits = {bar_split: []} if target_bars else {"valid": [], "test": []}
    if conditional_audit:
        splits = {"valid": [], "test": [], "train": []}
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
            if conditional_audit and conditional_audit_cohort(labels["normalizedLabel"]) is None:
                continue
            if only_indexed_products and not re.search(
                r"\\prod_\{[A-Za-z]\|[A-Za-z]\}", labels["normalizedLabel"]
            ):
                continue
            if only_indexed_divisibility and not indexed_divisibility(labels["normalizedLabel"]):
                continue
            if target_bars and bar_cohort(labels["normalizedLabel"], expanded_arithmetic) is None:
                continue
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
    if not all(splits.values()) or {r[0] for r in splits.get("valid", [])} & {r[0] for r in splits.get("test", [])}:
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

def compare_ranking(rows, paired):
    counts = dict(before_top1=0, after_top1=0, before_top3=0, after_top3=0,
                  changed=0, corrected=0, regressed=0, bar_labels=0, bar_labels_changed=0)
    for (_, label, _), prediction in zip(rows, paired):
        expected = formula(label)
        bar_label = any(token in {"|", r"\vert", r"\lvert", r"\rvert", r"\mid", r"\parallel", r"\|"} for token in expected)
        counts["bar_labels"] += bar_label
        if "error" in prediction:
            continue
        before, after = prediction["before"], prediction["after"]
        changed = before["latex"] != after["latex"]
        counts["changed"] += changed
        counts["bar_labels_changed"] += changed and bar_label
        matched = {}
        for key, value in (("before", before), ("after", after)):
            found = [c["complete"] and formula(c["latex"]) == expected
                     for c in value["candidates"][:3]]
            matched[key] = bool(found and found[0])
            counts[key + "_top1"] += matched[key]
            counts[key + "_top3"] += any(found)
        counts["corrected"] += not matched["before"] and matched["after"]
        counts["regressed"] += matched["before"] and not matched["after"]
    counts["wrong_promotions"] = counts["changed"] - counts["corrected"]
    n = counts["changed"]
    counts["promotion_rates"] = None
    if n:
        def proportion(hits):
            p = hits / n
            z = 1.95996398454
            denominator = 1 + z*z/n
            center = (p + z*z/(2*n)) / denominator
            radius = z * math.sqrt((p*(1-p) + z*z/(4*n))/n) / denominator
            return {"rate": p, "wilson_95": [max(0., center-radius), min(1., center+radius)]}
        counts["promotion_rates"] = {
            "correct": proportion(counts["corrected"]),
            "wrong": proportion(counts["wrong_promotions"]),
        }
    return counts


def slash_demotions(rows, paired):
    count = 0
    for (_, label, _), result in zip(rows, paired, strict=True):
        if "error" in result or "/" not in formula(label):
            continue
        before, after = result["before"], result["after"]
        expected = formula(label)
        if not before["complete"] or not after["complete"] or formula(before["latex"]) != expected:
            continue
        actual = formula(after["latex"])
        if len(expected) == len(actual) and [
            (a, b) for a, b in zip(expected, actual) if a != b
        ] == [("/", "|")]:
            count += 1
    return count


def slash_demotions_by_version(rows, paired):
    def against_unadjusted(version):
        return [{"before": r["without_rule"], "after": r[version]} if "error" not in r else r
                for r in paired]

    return {version: slash_demotions(rows, against_unadjusted(version))
            for version in ("before", "after")}


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
    parser.add_argument("--paired", action="store_true", help="Compare raw and notebook ranking on identical inference")
    parser.add_argument("--notebook", action="store_true",
                        help="Evaluate a labeled notebook JSONL file without fitting calibration")
    parser.add_argument("--target-bars", action="store_true",
                        help="Compare ranking on official test handwriting by digit/bar reference category")
    parser.add_argument("--bar-split", choices=("test", "valid", "train"), default="test",
                        help="MathWriting split for targeted ranking evaluation")
    parser.add_argument("--indexed-divisibility", action="store_true",
                        help="Select only divisibility constraints in sum and product indices")
    parser.add_argument("--expanded-arithmetic", action="store_true",
                        help="Include flat additions and subtractions with any digit or variable")
    parser.add_argument("--bar-cohort", choices=("conditional_probability", "flat_arithmetic",
                        "other_single_bar", "integer_divisibility", "explicit_divisibility_notation"))
    parser.add_argument("--diagnostic-rows", type=Path,
                        help="Write per-sample labels and paired candidates as local JSONL")
    parser.add_argument("--indexed-products", action="store_true",
                        help="Select products whose indices have a literal bar between two letters")
    parser.add_argument("--conditional-audit", action="store_true",
                        help="Audit the isolated slash rule on new densities and genuine slashes")
    parser.add_argument("--prior-conditional-binary", type=Path,
                        help="Compare against a previous benchmark executable using the same inputs")
    args = parser.parse_args()
    if args.prior_conditional_binary and not (args.conditional_audit or args.notebook or args.target_bars):
        parser.error("--prior-conditional-binary requires --conditional-audit, --notebook, or --target-bars")
    if args.conditional_audit and (args.notebook or args.target_bars or args.max_tokens
                                   or args.limit_per_split or args.exclude_excerpt):
        parser.error("--conditional-audit selects its own MathWriting cohorts")
    if args.indexed_products and (not args.target_bars or args.indexed_divisibility):
        parser.error("--indexed-products requires --target-bars and a separate run from --indexed-divisibility")
    if (args.bar_cohort or args.diagnostic_rows) and not args.target_bars:
        parser.error("--bar-cohort and --diagnostic-rows require --target-bars")
    if args.expanded_arithmetic and not args.target_bars:
        parser.error("--expanded-arithmetic requires --target-bars")
    if args.indexed_divisibility and not args.target_bars:
        parser.error("--indexed-divisibility requires --target-bars")
    if args.bar_split != "test" and not args.target_bars:
        parser.error("--bar-split requires --target-bars")
    if args.target_bars:
        if args.notebook or args.max_tokens or args.limit_per_split:
            parser.error("--target-bars uses fixed cohort filters and limits")
        args.paired = True
    if args.notebook:
        args.paired = True
        if args.max_tokens or args.limit_per_split or args.exclude_excerpt:
            parser.error("MathWriting selection options are unavailable in notebook mode")
    if any(value is not None and value < 1 for value in (args.max_tokens, args.limit_per_split)):
        parser.error("limits must be positive")
    excluded = {row[0] for rows in load(args.exclude_excerpt).values() for row in rows} if args.exclude_excerpt else set()
    if args.notebook:
        records = [json.loads(line) for line in args.archive.read_text().splitlines()]
        if not records:
            parser.error("Notebook dataset is empty")
        splits = {"test": [(str(i), row["label"], row["ink"]) for i, row in enumerate(records)]}
    elif args.conditional_audit:
        splits = load(args.archive, conditional_audit=True)
    elif args.target_bars:
        available = load(args.archive, excluded=excluded, target_bars=True,
                         bar_split=args.bar_split, only_indexed_divisibility=args.indexed_divisibility,
                         expanded_arithmetic=args.expanded_arithmetic, only_indexed_products=args.indexed_products)[args.bar_split]
        cohorts = {}
        for row in available:
            name = bar_cohort(row[1], args.expanded_arithmetic)
            if args.bar_cohort is None or args.bar_cohort == name:
                cohorts.setdefault(name, []).append(row)
        if not cohorts:
            parser.error("No samples matched the requested cohort")
        splits = {"test": [row for name, group in cohorts.items()
                           for row in (group if name == "flat_arithmetic" and args.expanded_arithmetic else group[:150])]}
    else:
        splits = load(args.archive, args.max_tokens, args.limit_per_split, excluded)
    print({split: len(samples) for split, samples in splits.items()}, flush=True)
    subprocess.run(["cargo", "build", "--locked", "--offline", "-p", "symbio-handwriting",
                    "--example", "benchmark"], cwd=ROOT, check=True)
    rows = [row for split in ("valid", "test", "train") for row in splits.get(split, [])]
    def infer(batch):
        input_rows = "".join(json.dumps(ink) + "\n" for _, _, ink in batch)

        def run(binary, mode):
            result = subprocess.run([binary, args.models, args.runtime, *mode],
                                    input=input_rows, text=True, stdout=subprocess.PIPE, check=True)
            predictions = [json.loads(line) for line in result.stdout.splitlines()]
            if len(predictions) != len(batch):
                raise ValueError("Recognizer output row count differs from input")
            return predictions

        current = ROOT / "target/debug/examples/benchmark"
        if args.prior_conditional_binary:
            before = run(args.prior_conditional_binary.resolve(), [])
            after = run(current, ["--conditional-paired"])
            return [
                {"before": a, "after": b["after"], "without_rule": b["before"]}
                if "error" not in a and "error" not in b
                else {"error": a.get("error") or b["error"]}
                for a, b in zip(before, after, strict=True)
            ]
        mode = ["--conditional-paired"] if args.conditional_audit else ["--paired"] if args.paired else []
        return run(current, mode)

    size = math.ceil(len(rows) / args.jobs)
    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        results = [result for batch in executor.map(infer, [
            rows[start:start + size] for start in range(0, len(rows), size)
        ]) for result in batch]
    if args.conditional_audit:
        report = {
            "archive_sha256": sha256(args.archive),
            "samples": len(rows),
            "inference_errors": sum("error" in result for result in results),
            "by_cohort": {},
        }
        if args.prior_conditional_binary:
            report["prior_binary_sha256"] = sha256(args.prior_conditional_binary)
        for name in sorted({conditional_audit_cohort(row[1]) for row in rows}):
            group = [(row, result) for row, result in zip(rows, results, strict=True)
                     if conditional_audit_cohort(row[1]) == name]
            cohort_rows, cohort_results = zip(*group)
            report["by_cohort"][name] = {
                "samples": len(group),
                **compare_ranking(cohort_rows, cohort_results),
                "correct_slash_to_bar": (
                    slash_demotions_by_version(cohort_rows, cohort_results)
                    if args.prior_conditional_binary else slash_demotions(cohort_rows, cohort_results)
                ),
            }
        with args.report.open("x") as output:
            json.dump(report, output, indent=2)
            output.write("\n")
        print(json.dumps(report, indent=2))
        return
    if args.diagnostic_rows:
        with args.diagnostic_rows.open("x") as output:
            for (sample_id, label, _), result in zip(rows, results, strict=True):
                output.write(json.dumps({"id": sample_id, "label": label,
                                         "cohort": bar_cohort(label, args.expanded_arithmetic),
                                         **result}, ensure_ascii=False) + "\n")
    if args.notebook or args.target_bars:
        report = {
            "dataset_sha256": sha256(args.archive),
            "samples": len(rows),
            "inference_errors": sum("error" in row for row in results),
            "ranking_comparison": compare_ranking(rows, results),
        }
        if args.prior_conditional_binary:
            report["prior_binary_sha256"] = sha256(args.prior_conditional_binary)
            report["correct_slash_to_bar"] = slash_demotions_by_version(rows, results)
        if args.target_bars:
            report["source_split"] = args.bar_split
            report["selected_cohort"] = args.bar_cohort
            report["expanded_arithmetic"] = args.expanded_arithmetic
            report["indexed_products_only"] = args.indexed_products
            report["indexed_divisibility_only"] = args.indexed_divisibility
            report["model_sha256"] = {
                name: sha256(args.models / name)
                for name in ("encoder_int8.onnx", "decoder_int8.onnx", "vocab.json")
            }
            report["runtime_sha256"] = sha256(args.runtime)
            report["changed_ids"] = [
                row[0] for row, prediction in zip(rows, results)
                if "error" not in prediction and prediction["before"]["latex"] != prediction["after"]["latex"]
            ]
            report["selected_ids_sha256"] = hashlib.sha256(
                "\n".join(row[0] for row in rows).encode()).hexdigest()
            report["by_cohort"] = {}
            for name, available in cohorts.items():
                subset = [(row, result) for row, result in zip(rows, results)
                          if bar_cohort(row[1], args.expanded_arithmetic) == name]
                cohort_rows, cohort_results = zip(*subset)
                report["by_cohort"][name] = {
                    "available": len(available), "samples": len(subset),
                    **compare_ranking(cohort_rows, cohort_results),
                }
        with args.report.open("x") as output:
            json.dump(report, output, indent=2)
            output.write("\n")
        print(json.dumps(report, indent=2))
        return
    paired = results if args.paired else None
    if paired is not None:
        results = [row.get("after", row) for row in paired]
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
    if paired is not None:
        comparisons = {}
        for split, start, end in (("valid", 0, cut), ("test", cut, len(rows))):
            comparisons[split] = compare_ranking(rows[start:end], paired[start:end])
        report["ranking_comparison"] = comparisons
        report["raw_candidate_calibration"] = candidate_calibration(
            rows, [row.get("before", row) for row in paired], cut)
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
