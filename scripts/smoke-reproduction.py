#!/usr/bin/env python3
"""End-to-end package creation/replay, changed expectation and no-clobber behavior."""
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
subprocess.run(["cargo", "build", "--locked", "--offline", "-p", "symbio-cli"], cwd=ROOT, check=True)
CLI = ROOT / "target/debug/symbio-cli"
with tempfile.TemporaryDirectory(prefix="symbio-reproduction-") as directory:
    source = Path(directory) / "study.symbio"
    package = Path(directory) / "study.symbio-repro.json"
    subprocess.run([str(CLI), "init", str(source)], check=True, capture_output=True)
    source_before = source.read_bytes()
    subprocess.run([str(CLI), "export-reproduction", str(source), str(package)], check=True, capture_output=True)
    report = json.loads(subprocess.run([str(CLI), "verify-reproduction", str(package)], check=True, capture_output=True, text=True).stdout)
    assert report["reproduced"] and report["matched"] == 2
    assert source.read_bytes() == source_before
    package_before = package.read_bytes()
    assert subprocess.run([str(CLI), "export-reproduction", str(source), str(package)], capture_output=True).returncode != 0
    assert package.read_bytes() == package_before
    altered = json.loads(package_before)
    altered["payload"]["cells"][0]["source"] = "x^2"
    package.write_text(json.dumps(altered))
    assert subprocess.run([str(CLI), "verify-reproduction", str(package)], capture_output=True).returncode != 0
    # Change only an expected digest, then recompute the format's checksum. A checksum is not authentication.
    altered = json.loads(package_before)
    altered["payload"]["cells"][0]["expected"]["output"]["expression_sha256"] = "0" * 64
    # Typed Rust payload serialization preserves field insertion order; maps in this fixture are already sorted.
    canonical = json.dumps(altered["payload"], ensure_ascii=False, separators=(",", ":")).encode()
    altered["sha256"] = hashlib.sha256(canonical).hexdigest()
    package.write_text(json.dumps(altered))
    result = subprocess.run([str(CLI), "verify-reproduction", str(package)], capture_output=True, text=True)
    assert result.returncode != 0
    report = json.loads(result.stdout)
    assert not report["reproduced"] and report["mismatched"] == 1
print("PASS: export/replay, immutable source, no-clobber, checksum rejection and recomputed-result mismatch")
