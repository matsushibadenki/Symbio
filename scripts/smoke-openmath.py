#!/usr/bin/env python3
"""Real CLI/XML roundtrip, explicit context, exact arithmetic and immutable destinations."""
import json
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET
ROOT=Path(__file__).resolve().parents[1]
subprocess.run(['cargo','build','--locked','--offline','-p','symbio-cli'],cwd=ROOT,check=True)
CLI=ROOT/'target/debug/symbio-cli'
def run(*args):return subprocess.run([CLI,*map(str,args)],capture_output=True,text=True)
with tempfile.TemporaryDirectory(prefix='symbio-openmath-') as directory:
    folder=Path(directory);source=ROOT/'examples/exact.openmath.xml';original=source.read_bytes();study=folder/'study.symbio';xml=folder/'exchange.xml'
    r=run('import-openmath',source,study);assert r.returncode==0,r.stderr
    result=json.loads(r.stdout)['cells'][0]['result_text'];assert result=='27021597764222980/3'
    assert source.read_bytes()==original
    assert run('export-openmath',study,xml).returncode==0
    tree=ET.fromstring(xml.read_bytes());ns={'om':'http://www.openmath.org/OpenMath'}
    assert tree.tag=='{http://www.openmath.org/OpenMath}OMOBJ'
    context=json.loads(tree.find('om:OMATTR/om:OMATP/om:OMSTR',ns).text)
    assert context['assumptions']=={} and context['version']==1
    r=run('import-openmath',xml,folder/'copy.symbio');assert r.returncode==0,r.stderr
    assert json.loads(r.stdout)['cells'][0]['result_text']==result
    before=xml.read_bytes();assert run('export-openmath',study,xml).returncode!=0;assert xml.read_bytes()==before
    before=study.read_bytes();assert run('import-openmath',xml,study).returncode!=0;assert study.read_bytes()==before
    model=folder/'model.json';model.write_text(json.dumps({'format':'symbio.mathjson','version':1,'name':'f','expression':['Sqrt',['Power','x',2]],'assumptions':{'x':{'domain':'real','nonzero':False,'dimension':None}}}))
    model_study=folder/'model.symbio';assert run('import-mathjson',model,model_study).returncode==0
    model_xml=folder/'model.xml';assert run('export-openmath',model_study,model_xml).returncode==0
    r=run('import-openmath',model_xml,folder/'model-copy.symbio');assert r.returncode==0,r.stderr
    value=json.loads(r.stdout);assert value['cells'][0]['result_text']=='abs(x)' and value['assumptions']['x']['domain']=='real'
    # Independent XML inspection plus a fresh process: exchange the authored
    # definite integral and its binder, not the cached rational result.
    calculus=ROOT/'examples/calculus.openmath.xml';calculus_before=calculus.read_bytes()
    r=run('import-openmath',calculus,folder/'calculus.symbio');assert r.returncode==0,r.stderr
    assert json.loads(r.stdout)['cells'][0]['result_text']=='8/3'
    calculus_xml=folder/'calculus.xml'
    assert run('export-openmath',folder/'calculus.symbio',calculus_xml).returncode==0
    tree=ET.fromstring(calculus_xml.read_bytes())
    assert tree.find('.//om:OMS[@cd="calculus1"][@name="defint"]',ns) is not None
    assert tree.find('.//om:OMS[@cd="interval1"][@name="oriented_interval"]',ns) is not None
    assert tree.find('.//om:OMBIND/om:OMBVAR/om:OMV',ns).attrib['name']=='x'
    assert tree.find('.//om:OMS[@cd="arith1"][@name="power"]',ns) is not None
    r=run('import-openmath',calculus_xml,folder/'calculus-copy.symbio');assert r.returncode==0,r.stderr
    assert json.loads(r.stdout)['cells'][0]['result_text']=='8/3'
    assert calculus.read_bytes()==calculus_before
    # Preserve matrix operations across both exchange formats, not cached answers.
    matrix_input=ROOT/'examples/transpose.openmath.xml';matrix_before=matrix_input.read_bytes()
    r=run('import-openmath',matrix_input,folder/'matrix.symbio');assert r.returncode==0,r.stderr
    matrix_result=json.loads(r.stdout)['cells'][0]['result_text']
    matrix_json=folder/'matrix.json'
    assert run('export-mathjson',folder/'matrix.symbio',matrix_json).returncode==0
    expression=json.loads(matrix_json.read_text())['expression']
    assert expression[0]=='Transpose' and expression[1][0]=='List'
    r=run('import-mathjson',matrix_json,folder/'matrix-copy.symbio');assert r.returncode==0,r.stderr
    assert json.loads(r.stdout)['cells'][0]['result_text']==matrix_result
    matrix_xml=folder/'matrix.xml'
    assert run('export-openmath',folder/'matrix-copy.symbio',matrix_xml).returncode==0
    assert ET.fromstring(matrix_xml.read_bytes()).find('.//om:OMS[@cd="linalg1"][@name="transpose"]',ns) is not None
    assert matrix_input.read_bytes()==matrix_before
    invalid=folder/'invalid.xml';invalid.write_text('<!DOCTYPE OMOBJ [<!ENTITY a SYSTEM "file:///etc/passwd">]><OMOBJ>&a;</OMOBJ>')
    assert run('import-openmath',invalid,folder/'invalid.symbio').returncode!=0;assert not (folder/'invalid.symbio').exists()
print('PASS: OpenMath/XML CLI roundtrip, exact large rational, semantic context, authored calculus binders, matrix operation cross-format exchange, immutable sources, no-clobber and rejected DTD')
