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
