#!/usr/bin/env python3
"""Real optional CAS solve, exact roots, all-factor completeness and recipe replay."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
ROOT=Path(__file__).resolve().parents[1]
subprocess.run(['cargo','build','--locked','--offline','-p','symbio-cli'],cwd=ROOT,check=True)
CLI=ROOT/'target/debug/symbio-cli'
env=dict(os.environ,SYMBIO_CAS_PYTHON=os.environ.get('SYMBIO_CAS_PYTHON',sys.executable))
with tempfile.TemporaryDirectory(prefix='symbio-cas-solve-') as directory:
    study=Path(directory)/'study.symbio';package=Path(directory)/'study.symbio-repro.json'
    subprocess.run([CLI,'init',study],check=True,capture_output=True)
    p=subprocess.Popen([CLI,'serve',study],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,env=env)
    revision=0
    def read(command):
        p.stdin.write(json.dumps(command)+'\n');p.stdin.flush();return json.loads(p.stdout.readline())
    def edit(command):
        global revision
        r=read({'expected_revision':revision,'actor':'human','command':command})
        if r['ok']:revision=r['result']['revision']
        return r
    def define(source):
        r=edit({'method':'math.define_latex','id':1,'name':'f','source':source});assert r['ok'],(source,r)
        return r
    def solve():return edit({'method':'math.transform','id':1,'operation':'solve_polynomial','variable':'x'})
    def roots(id=1):
        r=read({'method':'math.inspect','id':id})['result'];nodes={v['id']:v['node'] for v in r['nodes']}
        output=r['cell']['output'];assert output['trust']=='symbolic' and 'sympy/' in output['engine']
        node=nodes[output['root']];assert node['op']=='solutions'
        return [nodes[v] for v in node['args']['values']]
    for source,expected in [('x^4-1=0',{'-1','1'}),('x^6=1',{'-1','1'}),('(x-2)^5*(x+3)^2=0',{'2','-3'}),('x^3=x',{'-1','0','1'}),('7=0',set()),('x^2+1=0',set())]:
        define(source);r=solve();assert r['ok'],r
        values=roots();assert all(v['op']=='number' for v in values)
        assert {v['args'] for v in values}==expected,(source,values)
    define('x^4-5*x^2+6=0');assert solve()['ok'];assert len(roots())==4
    define('*'.join(f'(x-{i})' for i in range(32))+'=0');assert solve()['ok']
    assert {v['args'] for v in roots()}=={str(i) for i in range(32)}
    for source in ['x^8-1=0','(x-1)*(x^3-x-1)=0','x/x=1','x+y=0','0=0']:
        define(source);before=read({'method':'math.inspect'})['result'];r=solve();assert not r['ok'],(source,r)
        assert read({'method':'math.inspect'})['result']==before
    define('x^4-1=0');assert solve()['ok']
    r=edit({'method':'math.assume','symbol':'x','assumption':{'domain':'positive','nonzero':False,'dimension':None}});assert r['ok']
    assert {v['args'] for v in roots()}=={'1'}
    assert edit({'method':'math.assume','symbol':'x','assumption':{'domain':'real','nonzero':False,'dimension':None}})['ok']
    assert edit({'method':'math.define_latex','id':2,'name':'g','source':r'\cell{f}'})['ok']
    assert edit({'method':'math.transform','id':2,'operation':'solve_polynomial','variable':'x'})['ok']
    define('x^4-16=0');assert {v['args'] for v in roots(2)}=={'-2','2'}
    assert edit({'method':'math.undo'})['ok'];assert {v['args'] for v in roots(2)}=={'-1','1'}
    assert edit({'method':'math.redo'})['ok'];assert {v['args'] for v in roots(2)}=={'-2','2'}
    define('x^8-1=0');r=read({'method':'math.inspect'})['result'];assert all(c['output'] is None for c in r['cells'])
    define('x^4-16=0');assert read({'method':'math.save'})['ok'];p.stdin.close();assert p.wait()==0
    subprocess.run([CLI,'export-reproduction',study,package],env=env,capture_output=True,check=True)
    r=subprocess.run([CLI,'verify-reproduction',package],env=env,capture_output=True,text=True)
    assert r.returncode!=0 and json.loads(r.stdout)['skipped']==2
    r=subprocess.run([CLI,'verify-reproduction',package,'--allow-cas'],env=env,capture_output=True,text=True)
    assert r.returncode==0 and json.loads(r.stdout)['reproduced'],r.stdout+r.stderr
    missing=dict(env,SYMBIO_CAS_PYTHON='/nonexistent/python')
    r=subprocess.run([CLI,'inspect',study],env=missing,capture_output=True,text=True,check=True)
    assert all(c['output'] for c in json.loads(r.stdout)['cells'])
    r=subprocess.run([CLI,'verify-reproduction',package,'--allow-cas'],env=missing,capture_output=True,text=True)
    assert r.returncode!=0 and 'cas_unavailable' in r.stdout
print('PASS: real CAS exact polynomial roots, no partial answers, rollback, sign filtering, dependencies, Undo/Redo, saved cache and opt-in replay')
