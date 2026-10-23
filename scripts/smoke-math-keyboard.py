"""Actual palette data -> editing rules -> Rust CLI -> SEG -> exact computation."""
import json
import subprocess
import tempfile
from pathlib import Path
root=Path(__file__).resolve().parents[1]
subprocess.run(['cargo','build','--locked','--offline','-p','symbio-cli'],cwd=root,check=True)
js=r'''
import {computationalLatexGroups as keys} from './apps/desktop/ui/math-keyboard-layout.js';
import {editSelection,nextPlaceholder} from './apps/desktop/ui/math-keyboard-edit.js';
const wrap=(key,source)=>editSelection(source,0,source.length,{code:key.c},'latex').value;
let matrix=editSelection('',0,0,{code:keys.matrices[0].c},'latex');
for(const number of ['1','2','3','4']) {matrix=editSelection(matrix.value,matrix.start,matrix.end,{code:number},'latex');const p=nextPlaceholder(matrix.value,matrix.start,matrix.end);if(p)matrix={value:matrix.value,...p};}
console.log(JSON.stringify([
{source:wrap(keys.calculus[0],'x^{2}'),operation:'compute',expected:'1/3'},
{source:wrap(keys.calculus[2],'x^{3}'),operation:'compute',expected:'(3 * (x ^ 2))'},
{source:matrix.value,operation:'determinant',expected:'-2'},
{source:'x^{2}-5x+6=0',operation:'solve',expected:'x in {2, 3}'}
]));
'''
fixtures=json.loads(subprocess.run(['node','--input-type=module','-'],cwd=root,input=js,text=True,capture_output=True,check=True).stdout)
cli=root/'target/debug/symbio-cli'
with tempfile.TemporaryDirectory(prefix='symbio-keyboard-') as d:
    path=str(Path(d)/'test.symbio')
    subprocess.run([cli,'init',path],capture_output=True,check=True)
    commands=[]
    revision=0
    for fixture in fixtures:
        commands.extend([
            {'method':'math.preview_latex','source':fixture['source']},
            {'expected_revision':revision,'actor':'human','command':{'method':'math.define_latex','id':1,'name':'f','source':fixture['source']}},
            {'expected_revision':revision+1,'actor':'human','command':{'method':'math.transform','id':1,'operation':fixture['operation'],'variable':'x','bindings':{}}}
        ])
        revision+=2
    commands.append({'method':'math.save'})
    result=subprocess.run([cli,'serve',path],input='\n'.join(map(json.dumps,commands))+'\n',capture_output=True,text=True,check=True)
    rows=[json.loads(line) for line in result.stdout.splitlines()]
    assert all(r['ok'] for r in rows),rows
    for i,fixture in enumerate(fixtures):
        assert rows[i*3+2]['result']['cells'][0]['result_text']==fixture['expected'],rows[i*3+2]
    saved=json.loads(subprocess.run([cli,'inspect',path],capture_output=True,text=True,check=True).stdout)
    assert saved['cells'][0]['result_text']=='x in {2, 3}'
print('PASS: keyboard templates -> preview -> SEG -> integral, derivative, determinant, equation solutions -> save/reload')
