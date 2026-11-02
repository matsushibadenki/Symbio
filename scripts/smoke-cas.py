#!/usr/bin/env python3
"""Opt-in local SymPy transport, transactional recipes and safe reproduction defaults."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
subprocess.run(['cargo', 'build', '--locked', '--offline', '-p', 'symbio-cli'], cwd=ROOT, check=True)
CLI = ROOT / 'target/debug/symbio-cli'
PYTHON = os.environ.get('SYMBIO_CAS_PYTHON', sys.executable)
subprocess.run([PYTHON, '-I', '-c', 'import sympy'], check=True)
env = dict(os.environ, SYMBIO_CAS_PYTHON=PYTHON)
with tempfile.TemporaryDirectory(prefix='symbio-cas-') as directory:
    path = Path(directory) / 'study.symbio'
    subprocess.run([CLI, 'init', path], check=True, capture_output=True)
    process = subprocess.Popen([CLI, 'serve', path], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env)
    revision = 0
    def request(command):
        global revision
        process.stdin.write(json.dumps({'expected_revision':revision,'actor':'human','command':command})+'\n')
        process.stdin.flush()
        row = json.loads(process.stdout.readline())
        if row['ok']: revision = row['result']['revision']
        return row
    for source in ['x^4-1', '(x-2)^3/3', 'x^2+1', '0', '7/3']:
        assert request({'method':'math.define','id':1,'name':'f','source':source})['ok']
        row = request({'method':'math.transform','id':1,'operation':'factor','variable':'x'})
        assert row['ok'], row
        cell = row['result']['cells'][0]
        assert cell['output']['trust']=='symbolic' and 'sympy/' in cell['output']['engine']
        assert 'coefficient identity checked in Rust' in cell['output']['conditions'][-1]
    row = request({'method':'math.define','id':1,'name':'f','source':'x^4-1'})
    assert row['result']['cells'][0]['output']['operation']=='factor', row
    assert '(x - 1)' in row['result']['cells'][0]['result_text'], row
    assert request({'method':'math.define','id':2,'name':'g','source':'@f'})['ok']
    row=request({'method':'math.transform','id':2,'operation':'factor','variable':'x'})
    assert row['ok'] and row['result']['cells'][1]['output']['operation']=='factor'
    row=request({'method':'math.define','id':1,'name':'f','source':'x^2-1'})
    assert row['ok'] and row['result']['cells'][1]['result_text']=='((x - 1) * (x + 1))', row
    assert request({'method':'math.undo'})['ok']
    row=request({'method':'math.redo'})
    assert row['ok'] and row['result']['cells'][1]['result_text']=='((x - 1) * (x + 1))', row
    for source in ['x+y', 'x/x']:
        changed=request({'method':'math.define','id':1,'name':'f','source':source})
        assert changed['ok'] and changed['result']['cells'][1]['output'] is None, changed
        # Recalculation must fail rather than cancel singularities or manufacture a result.
        row = request({'method':'math.transform','id':1,'operation':'factor','variable':'x'})
        assert not row['ok'], row
    assert request({'method':'math.define','id':1,'name':'f','source':'x^4-1'})['ok']
    process.stdin.write('{"method":"math.save"}\n');process.stdin.flush()
    assert json.loads(process.stdout.readline())['ok']
    process.stdin.close();assert process.wait()==0
    package=Path(directory)/'study.symbio-repro.json'
    subprocess.run([CLI,'export-reproduction',path,package],check=True,capture_output=True,env=env)
    default=subprocess.run([CLI,'verify-reproduction',package],capture_output=True,text=True,env=env)
    report=json.loads(default.stdout)
    assert not report['reproduced'] and any('external_runtime' in c['differences'] for c in report['cells']),report
    replay=subprocess.run([CLI,'verify-reproduction',package,'--allow-cas'],capture_output=True,text=True,env=env)
    assert replay.returncode==0 and json.loads(replay.stdout)['reproduced'], replay.stdout+replay.stderr
    missing=dict(env,SYMBIO_CAS_PYTHON='/nonexistent/python')
    result=subprocess.run([CLI,'verify-reproduction',package,'--allow-cas'],capture_output=True,text=True,env=missing)
    assert result.returncode!=0 and 'cas_unavailable' in result.stdout
print('PASS: local SymPy, exact factor identity, recipe recalculation, unsupported scope, opt-in replay and unavailable runtime')
