import test from 'node:test';
import assert from 'node:assert/strict';
import {editSelection,eraseSelection,DraftHistory} from '../apps/desktop/ui/math-keyboard-edit.js';
import {referenceKeys} from '../apps/desktop/ui/math-keyboard-data.js';

test('fraction wraps the selected sum and selects its denominator',()=>{
 const out=editSelection('x+1',0,3,{template:'fraction'});
 assert.deepEqual(out,{value:'(x+1)/(1)',start:7,end:8});
 const filled=editSelection(out.value,out.start,out.end,{code:'2'});
 assert.equal(filled.value,'(x+1)/(2)');
});
test('root and power preserve selected compound expression',()=>{
 assert.equal(editSelection('1+x+2',2,5,{wrap:'sqrt($)'}).value,'1+sqrt(x+2)');
 const power=editSelection('x+1',0,3,{template:'power'});
 assert.equal(power.value,'(x+1)^(2)');
 assert.equal(power.value.slice(power.start,power.end),'2');
});
test('empty root places the caret on an editable kernel-compatible argument',()=>{
 assert.deepEqual(editSelection('',0,0,{wrap:'sqrt($)'}),{value:'sqrt(x)',start:5,end:6});
});
test('original fraction and integral data preserve LaTeX placeholders',()=>{
 const fraction=referenceKeys.find(k=>k.s==='a⁄b');
 const result=editSelection('x+1',0,3,{code:fraction.c},'latex');
 assert.equal(result.value,'\\frac{x+1}{□}');
 assert.equal(result.value.slice(result.start,result.end),'□');
 const integral=editSelection('',0,0,{code:referenceKeys.find(k=>k.s==='∫').c},'latex');
 assert.equal(integral.value,'\\int_{□}^{□} ');
 assert.equal(integral.start,6);
});
test('LaTeX powers retain the selected base and literal dollar text',()=>{
 assert.equal(editSelection('x',0,1,{code:'^{}'},'latex').value,'x^{□}');
 const power=editSelection('x+1',0,3,{code:'^{}'},'latex');
 assert.equal(power.value,'\\left(x+1\\right)^{□}');
 assert.equal(power.value.slice(power.start,power.end),'□');
 assert.equal(editSelection('$&',0,2,{code:'\\sqrt{}'},'latex').value,'\\sqrt{$&}');
});
test('delete and backspace handle selection, Unicode and boundaries',()=>{
 assert.deepEqual(eraseSelection('a𝛼b',1,3,false),{value:'ab',start:1,end:1});
 assert.deepEqual(eraseSelection('a𝛼b',3,3,true),{value:'ab',start:1,end:1});
 assert.equal(eraseSelection('abc',0,0,true).value,'abc');
 assert.equal(eraseSelection('abc',3,3,false).value,'abc');
});
test('draft undo branches and is bounded independently of document history',()=>{
 const h=new DraftHistory('x');h.record('sqrt(x)');h.record('sqrt(2)');
 assert.equal(h.move(-1),'sqrt(x)');assert.equal(h.move(1),'sqrt(2)');
 h.move(-1);h.record('cos(x)');assert.equal(h.move(1),'cos(x)');
 for(let i=0;i<100;i++)h.record(String(i));assert.equal(h.entries.length,32);
});
