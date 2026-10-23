"""Exercise the real JSONL process, persistence, actor attribution and rejected edits."""
import json
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
subprocess.run(["cargo", "build", "--locked", "-p", "symbio-cli"], cwd=root, check=True)
cli = root / "target/debug/symbio-cli"
with tempfile.TemporaryDirectory(prefix="symbio-smoke-") as directory:
    path = str(Path(directory) / "study.symbio")
    subprocess.run([cli, "init", path], check=True, capture_output=True, text=True)
    session = (root / "examples/session.jsonl").read_text()
    result = subprocess.run([cli, "serve", path], input=session, capture_output=True, text=True, check=True)
    rows = [json.loads(line) for line in result.stdout.splitlines()]
    assert len(rows) == 7 and all(r["ok"] for r in rows), rows
    assert rows[4]["result"]["cells"][1]["result_text"] == "x"
    saved = json.loads(subprocess.run([cli, "inspect", path], capture_output=True, text=True, check=True).stdout)
    assert saved["active"] == "positive-x" and saved["revision"] == 4
    assert any(h["actor"] == "ai" for h in saved["history"])
    errors = [
        {"expected_revision": 0, "actor": "ai", "command": {"method": "math.note", "id": 1, "text": "stale"}},
        {"expected_revision": 4, "actor": "human", "command": {"method": "math.define", "id": 1, "name": "f", "source": "x+"}},
        {"method": "math.inspect"},
    ]
    result = subprocess.run([cli, "serve", path], input="\n".join(map(json.dumps, errors))+"\n", capture_output=True, text=True, check=True)
    rows = [json.loads(line) for line in result.stdout.splitlines()]
    assert not rows[0]["ok"] and not rows[1]["ok"]
    assert rows[2]["result"]["revision"] == 4
    assert rows[2]["result"]["cells"][0]["source"] == saved["cells"][0]["source"]
    commands = [
        {"method":"math.preview_latex", "source":r"\frac{x^2}{2}"},
        {"expected_revision":4,"actor":"human","command":{"method":"math.define_latex","id":1,"name":"f","source":r"\frac{x^2}{2}"}},
        {"expected_revision":5,"actor":"human","command":{"method":"math.transform","id":1,"operation":"differentiate","variable":"x","bindings":{}}},
        {"method":"math.save"},
    ]
    result = subprocess.run([cli,"serve",path],input="\n".join(map(json.dumps,commands))+"\n",capture_output=True,text=True,check=True)
    rows = [json.loads(line) for line in result.stdout.splitlines()]
    assert all(r["ok"] for r in rows), rows
    assert "<mfrac>" in rows[0]["result"]["mathml"]
    assert rows[2]["result"]["cells"][0]["result_text"] == "x"
    loaded = json.loads(subprocess.run([cli,"inspect",path],capture_output=True,text=True,check=True).stdout)
    assert loaded["cells"][0]["source_format"] == "latex"
    assert loaded["cells"][0]["source"] == r"\frac{x^2}{2}"
    assert loaded["revision"] == 6
print("PASS: JSONL session, AI attribution, save/reopen, conflict and parse-error rollback, LaTeX preview/SEG/differentiation/reload")
