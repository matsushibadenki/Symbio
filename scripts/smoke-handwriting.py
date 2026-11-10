#!/usr/bin/env python3
"""Real Rust ONNX model → recognized LaTeX → Symbio SEG, using owned fixtures.
Usage: smoke-handwriting.py MODELS RUNTIME
"""
from pathlib import Path
import json
import subprocess
import sys
import tempfile
ROOT=Path(__file__).resolve().parents[1]
if len(sys.argv) not in (3,4) or (len(sys.argv)==4 and sys.argv[3]!='--beam'): raise SystemExit('usage: smoke-handwriting.py MODELS RUNTIME [--beam]')
subprocess.run(['cargo','build','--locked','--offline','-p','symbio-handwriting','--example','recognize','-p','symbio-cli'],cwd=ROOT,check=True)
recognizer=ROOT/'target/debug/examples/recognize';cli=ROOT/'target/debug/symbio-cli'
with tempfile.TemporaryDirectory(prefix='symbio-handwriting-smoke-') as d:
 project=Path(d)/'study.symbio'
 subprocess.run([cli,'init',project],capture_output=True,text=True,check=True)
 revision=0
 for name,expected in [('one','1'),('zero','0'),('x-plus-one','x + 1')]:
  r=subprocess.run([recognizer,sys.argv[1],sys.argv[2],ROOT/f'examples/handwriting/{name}.json',*sys.argv[3:]],capture_output=True,text=True,check=True)
  result=json.loads(r.stdout)
  candidates=result['candidates'];assert 1<=len(candidates)<=3
  assert len({c['latex'] for c in candidates})==len(candidates)
  assert all(c['log_score']<=0 for c in candidates)
  assert candidates[0]['latex']==result['latex']
  assert all(a['log_score']>=b['log_score'] for a,b in zip(candidates,candidates[1:]))
  assert result['complete'] and result['latex']==expected,result
  request={'expected_revision':revision,'actor':'human','command':{'method':'math.define_latex','id':None,'name':'ink'+str(revision),'source':result['latex']}}
  r=subprocess.run([cli,'serve',project],input=json.dumps(request)+'\n'+json.dumps({'method':'math.save'})+'\n',capture_output=True,text=True,check=True)
  response=json.loads(r.stdout.splitlines()[0]);assert response['ok'],response
  cell=response['result']['cells'][-1];assert cell['output']['trust']=='symbolic';assert cell['output']['precision']=='exact'
  if name!='x-plus-one': assert cell['result_text']==expected
  revision=response['result']['revision']
  print('PASS:',name,'→',result['latex'],'→ SEG /',result['elapsed_ms'],'ms')
print('PASS: 3 owned synthetic stroke fixtures, native ONNX inference, clean process exit and real CLI SEG calculation')

r=subprocess.run([recognizer,sys.argv[1],sys.argv[2],ROOT/'examples/handwriting/x-plus-one.json','--cancel'],capture_output=True,text=True,check=True)
result=json.loads(r.stdout)
assert result['cancelled'] and result['retry']['complete'] and result['retry']['latex']=='x + 1',result
print('PASS: pre-cancelled native request rejected; same recognizer accepts a fresh retry')
