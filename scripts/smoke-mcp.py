#!/usr/bin/env python3
"""Real stdio client plus JSON Schema validation; requires local jsonschema for this test only."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import jsonschema
ROOT=Path(__file__).resolve().parents[1]
subprocess.run(['cargo','build','--locked','--offline','-p','symbio-cli'],cwd=ROOT,check=True)
CLI=ROOT/'target/debug/symbio-cli'
with tempfile.TemporaryDirectory(prefix='symbio-mcp-') as directory:
    folder=Path(directory);marker=folder/'executed'
    fake=folder/'python';fake.write_text('#!/bin/sh\ntouch "'+str(marker)+'"\nexit 1\n');fake.chmod(0o700)
    env=dict(os.environ,SYMBIO_CAS_PYTHON=str(fake))
    process=subprocess.Popen([CLI,'mcp'],cwd=folder,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    number=0
    def send(v):
        process.stdin.write(json.dumps(v)+'\n');process.stdin.flush()
    def rpc(method,params=None):
        global number
        number+=1;send({'jsonrpc':'2.0','id':number,'method':method,'params':params or {}})
        row=json.loads(process.stdout.readline());assert row['id']==number,row;return row
    assert rpc('tools/list')['error']['code']==-32000
    r=rpc('initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'symbio-smoke','version':'1'}})
    assert r['result']['protocolVersion']=='2025-11-25'
    assert r['result']['capabilities']=={'tools':{'listChanged':False}}
    send({'jsonrpc':'2.0','method':'notifications/initialized'})
    descriptions=rpc('tools/list')['result']['tools'];schemas={t['name']:t for t in descriptions}
    assert set(schemas)=={'math_calculate','math_preview_latex'}
    for tool in descriptions:
        jsonschema.Draft202012Validator.check_schema(tool['inputSchema'])
        jsonschema.Draft202012Validator.check_schema(tool['outputSchema'])
        assert tool['annotations']['readOnlyHint'] and not tool['annotations']['openWorldHint']
    def call(name,args):
        jsonschema.validate(args,schemas[name]['inputSchema'])
        r=rpc('tools/call',{'name':name,'arguments':args})['result']
        jsonschema.validate(r['structuredContent'],schemas[name]['outputSchema'])
        assert json.loads(r['content'][0]['text'])==r['structuredContent'];return r
    r=call('math_calculate',{'source':r'\int_0^2 x^2\,dx','source_format':'latex'})
    assert not r['isError'] and r['structuredContent']['cell']['result_text']=='8/3'
    assert r['structuredContent']['cell']['output']['trust']=='symbolic'
    r=call('math_calculate',{'source':'sqrt(x^2)','assumptions':{'x':{'domain':'real'}}})
    assert r['structuredContent']['cell']['result_text']=='abs(x)'
    r=call('math_calculate',{'source':'sqrt(x^2)'})
    assert r['structuredContent']['cell']['result_text']!='abs(x)'
    r=call('math_calculate',{'source':'x^2','operation':'evaluate','bindings':{'x':2}})
    assert r['structuredContent']['cell']['output']['trust']=='approximate'
    r=call('math_calculate',{'source':'x^2','operation':'substitute','exact_bindings':{'x':'1/3'}})
    assert r['structuredContent']['cell']['result_text']=='1/9'
    assert r['structuredContent']['cell']['output']['precision']=='exact'
    for args in [{'source':'x','operation':'substitute'},
                 {'source':'x','operation':'substitute','exact_bindings':{'x':1}},
                 {'source':'x','operation':'evaluate','exact_bindings':{'x':'1'}}]:
        assert rpc('tools/call',{'name':'math_calculate','arguments':args})['error']['code']==-32602
    r=call('math_preview_latex',{'source':r'\begin{bmatrix}1&2\\3&4\end{bmatrix}'})
    assert r['structuredContent']['preview']['kind']=='matrix'
    for args in [{'source':'@f'},{'source':'1/0'},{'source':'x+'}]:
        r=call('math_calculate',args);assert r['isError'] and not r['structuredContent']['ok']
    for args in [{'source':'x','operation':'factor'},{'source':'x','path':'/tmp/write'},{'source':'x','source_format':'python'}]:
        assert rpc('tools/call',{'name':'math_calculate','arguments':args})['error']['code']==-32602
    send({'jsonrpc':'2.0','method':'notifications/cancelled','params':{'requestId':0}})
    assert rpc('ping')['result']=={}
    assert not marker.exists() and set(folder.iterdir())=={fake}
    process.stdin.close();assert process.wait(timeout=10)==0
    assert not process.stderr.read()
    # Oversized frames terminate without processing a following command.
    r=subprocess.run([CLI,'mcp'],input='x'*65537+'\n',capture_output=True,text=True,env=env)
    assert r.returncode!=0 and not r.stdout and '64 KiB' in r.stderr
print('PASS: real MCP stdio lifecycle, schema validation, exact calculus, evidence, isolation, rejected authority, no CAS/file writes, framing limits')

with tempfile.TemporaryDirectory(prefix="symbio-mcp-notebook-") as directory:
    path = Path(directory) / "study.symbio"
    subprocess.run([CLI, "init", path], capture_output=True, check=True)
    changes = [
        {"expected_revision": 0, "actor": "human",
         "command": {"method": "math.note", "id": 1, "text": "private note"}},
        {"method": "math.save"},
    ]
    saved = subprocess.run([CLI, "serve", path], text=True, capture_output=True, check=True,
                           input="".join(json.dumps(row) + "\n" for row in changes))
    assert all(json.loads(line)["ok"] for line in saved.stdout.splitlines())
    path.with_suffix(".symbio.lock").unlink()
    original = path.read_bytes()
    with subprocess.Popen([CLI, "mcp", "--read-project", path], text=True,
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE) as process:
        rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                           "clientInfo": {"name": "symbio-smoke", "version": "1"}})
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        schema = next(tool for tool in rpc("tools/list")["result"]["tools"]
                      if tool["name"] == "math_read_notebook")
        for args in [{}, {"cell_id": 1}, {"cell_id": 999}]:
            response = rpc("tools/call", {"name": schema["name"], "arguments": args})["result"]
            content = response["structuredContent"]
            jsonschema.validate(args, schema["inputSchema"])
            jsonschema.validate(content, schema["outputSchema"])
            assert "private note" not in json.dumps(content)
            if not args:
                assert content["revision"] == 1 and content["cells"][0]["id"] == 1
            elif args["cell_id"] == 1:
                assert content["cell"]["source"]
                assert "note" not in content["cell"] and "history" not in content
            else:
                assert response["isError"] and content["error"]["code"] == "cell_not_found"
        for args in [{"path": str(path)}, {"cell_id": 0}, {"cell_id": None}]:
            assert rpc("tools/call", {"name": schema["name"], "arguments": args})["error"]["code"] == -32602
        process.stdin.close()
        assert process.wait(timeout=10) == 0
    assert path.read_bytes() == original and list(Path(directory).iterdir()) == [path]
print("PASS: opt-in MCP notebook read, schema, data boundaries and no file changes")

with tempfile.TemporaryDirectory(prefix="symbio-mcp-branch-") as directory:
    path = Path(directory) / "study.symbio"
    subprocess.run([CLI, "init", path], capture_output=True, check=True)
    commands = [
        {"method": "math.branch", "name": "real study"},
        {"method": "math.assume", "symbol": "x", "assumption": {"domain": "positive"}},
        {"method": "math.checkout", "name": "main"},
    ]
    requests = [{"expected_revision": rev, "actor": "human", "command": command}
                for rev, command in enumerate(commands)]
    requests.append({"method": "math.save"})
    saved = subprocess.run([CLI, "serve", path], text=True, capture_output=True, check=True,
                           input="".join(json.dumps(row) + "\n" for row in requests))
    assert all(json.loads(line)["ok"] for line in saved.stdout.splitlines())
    original = path.read_bytes()
    with subprocess.Popen([CLI, "mcp", "--read-project", path, "--branch", "real study"], text=True,
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE) as process:
        rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                           "clientInfo": {"name": "symbio-smoke", "version": "1"}})
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        content = rpc("tools/call", {"name": "math_read_notebook"})["result"]["structuredContent"]
        assert content["branch"] == "real study" and content["assumptions"]["x"]["domain"] == "positive"
        assert "main" not in content
        blocked = rpc("tools/call", {"name": "math_read_notebook", "arguments": {"branch": "main"}})
        assert blocked["error"]["code"] == -32602
        process.stdin.close()
        assert process.wait(timeout=10) == 0
    assert json.loads(path.read_bytes())["active"] == "main" and path.read_bytes() == original
    unknown = subprocess.run([CLI, "mcp", "--read-project", path, "--branch", "missing"],
                             capture_output=True, text=True)
    assert unknown.returncode != 0 and not unknown.stdout and "does not exist" in unknown.stderr
print("PASS: explicit saved branch selection and startup authorization boundary")

with tempfile.TemporaryDirectory(prefix="symbio-mcp-follow-") as directory:
    path = Path(directory) / "study.symbio"
    subprocess.run([CLI, "init", path], capture_output=True, check=True)
    with subprocess.Popen([CLI, "mcp", "--read-project", path, "--follow-saves"], text=True,
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE) as process:
        rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                           "clientInfo": {"name": "symbio-smoke", "version": "1"}})
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})

        def read_cell():
            return rpc("tools/call", {"name": "math_read_notebook",
                                      "arguments": {"cell_id": 1}})["result"]["structuredContent"]

        def search(**arguments):
            return rpc("tools/call", {"name": "math_read_notebook",
                                      "arguments": arguments})["result"]["structuredContent"]

        found = search(query="F")
        assert found["cells"] == [{"id": 1, "name": "f"}] and "assumptions" not in found
        assert search(query="unknown")["cells"] == []
        assert read_cell()["revision"] == 0
        commands = [
            {"method": "math.define", "id": 1, "name": "f", "source": "x+42"},
            {"method": "math.branch", "name": "other"},
            {"method": "math.define", "id": 1, "name": "f", "source": "99"},
        ]
        requests = [{"expected_revision": rev, "actor": "human", "command": command}
                    for rev, command in enumerate(commands)]
        requests.append({"method": "math.save"})
        saved = subprocess.run([CLI, "serve", path], text=True, capture_output=True, check=True,
                               input="".join(json.dumps(row) + "\n" for row in requests))
        assert all(json.loads(line)["ok"] for line in saved.stdout.splitlines())
        fresh = read_cell()
        assert fresh["branch"] == "main" and fresh["revision"] == 3
        assert fresh["cell"]["source"] == "x+42"
        assert search(cell_id=1, expected_revision=found["revision"])["error"]["code"] == "revision_conflict"
        assert search(cell_id=1, expected_revision=fresh["revision"])["cell"]["source"] == "x+42"
        replacement = path.with_name("different.symbio")
        subprocess.run([CLI, "init", replacement], capture_output=True, check=True)
        replacement.replace(path)
        assert read_cell() == {"ok": False, "error": {"code": "notebook_unavailable"}}
        process.stdin.close()
        assert process.wait(timeout=10) == 0
print("PASS: saved revision refresh stays pinned to the authorized branch and project")
