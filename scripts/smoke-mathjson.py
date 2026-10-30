#!/usr/bin/env python3
"""MathJSON CLI roundtrip, exact arithmetic, context and destination preservation."""
import json
import subprocess
import tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
subprocess.run(["cargo","build","--locked","--offline","-p","symbio-cli"],cwd=ROOT,check=True)
CLI=ROOT/"target/debug/symbio-cli"
def run(*args):
    return subprocess.run([str(CLI),*map(str,args)],capture_output=True,text=True)
with tempfile.TemporaryDirectory(prefix="symbio-mathjson-") as folder:
    folder=Path(folder);source=folder/"input.json";project=folder/"study.symbio";exchange=folder/"exchange.json";copy=folder/"copy.symbio"
    source.write_text(json.dumps(["Add",{"num":"9007199254740993"},["Rational",1,3]]))
    original=source.read_bytes();r=run("import-mathjson",source,project);assert r.returncode==0,r.stderr
    snapshot=json.loads(r.stdout);assert snapshot["cells"][0]["result_text"]=="27021597764222980/3"
    assert source.read_bytes()==original
    assert run("export-mathjson",project,exchange).returncode==0
    v=json.loads(exchange.read_text());assert v["format"]=="symbio.mathjson" and "9007199254740993" in exchange.read_text()
    r=run("import-mathjson",exchange,copy);assert r.returncode==0,r.stderr
    assert json.loads(r.stdout)["cells"][0]["result_text"]==snapshot["cells"][0]["result_text"]
    before=project.read_bytes();assert run("import-mathjson",exchange,project).returncode!=0;assert project.read_bytes()==before
    before=exchange.read_bytes();assert run("export-mathjson",project,exchange).returncode!=0;assert exchange.read_bytes()==before
    context=folder/"context.json";context.write_text(json.dumps({"format":"symbio.mathjson","version":1,"name":"root","expression":["Sqrt",["Power","x",2]],"assumptions":{"x":{"domain":"nonnegative","nonzero":False,"dimension":None}}}))
    r=run("import-mathjson",context,folder/"context.symbio");assert r.returncode==0,r.stderr;assert json.loads(r.stdout)["cells"][0]["result_text"]=="x"
    source.write_text('["Log","x"]');assert run("import-mathjson",source,folder/"unsupported.symbio").returncode!=0;assert not (folder/"unsupported.symbio").exists()
print("PASS: exact MathJSON roundtrip, explicit assumptions, immutable inputs, no-clobber and unsupported head rejection")
