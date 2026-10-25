#!/usr/bin/env python3
"""Bounded JSONL integration check: compare without checkout, mutation or save."""
import json
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
subprocess.run(["cargo", "build", "--locked", "--offline", "-p", "symbio-cli"], cwd=ROOT, check=True)
CLI = ROOT / "target/debug/symbio-cli"
with tempfile.TemporaryDirectory(prefix="symbio-comparison-") as directory:
    path = Path(directory) / "comparison.symbio"
    subprocess.run([str(CLI), "init", str(path)], check=True, capture_output=True)
    before_file = path.read_bytes()
    requests = [
        {"expected_revision": 0, "actor": "human", "command": {"method": "math.branch", "name": "trial"}},
        {"expected_revision": 1, "actor": "human", "command": {"method": "math.define", "id": 1, "name": "f", "source": "x^2"}},
        {"method": "math.inspect"},
        {"method": "math.compare_branches", "base": "main", "target": "trial"},
        {"method": "math.inspect"},
        {"method": "math.compare_branches", "base": "trial", "target": "main"},
        {"method": "math.compare_branches", "base": "missing", "target": "trial"},
        {"method": "math.compare_branches", "base": "main", "target": "trial", "unexpected": True},
        {"method": "math.inspect"},
    ]
    output = subprocess.run([str(CLI), "serve", str(path)], input="".join(json.dumps(r)+"\n" for r in requests), text=True, capture_output=True, check=True)
    rows = [json.loads(line) for line in output.stdout.splitlines()]
    assert len(rows) == len(requests)
    assert all(row["ok"] for row in rows[:6])
    diff = rows[3]["result"]
    assert diff["base"] == "main" and diff["target"] == "trial"
    assert "definition" in diff["cells"][0]["fields"]
    assert rows[2]["result"] == rows[4]["result"] == rows[8]["result"]
    assert not rows[6]["ok"] and not rows[7]["ok"]
    assert path.read_bytes() == before_file
print("PASS: branch JSONL comparison, reverse direction, missing/unknown-field errors, unchanged state and file")
