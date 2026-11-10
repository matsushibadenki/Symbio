import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';

// Exercise the real editor's async event flow; only DOM/MathLive and host IPC are stubs.
const code=(await readFile(new URL('../apps/desktop/ui/handwriting.js',import.meta.url),'utf8'))
 .replace('export async function','async function')
 .replace("await import('./vendor/mathlive/mathlive.min.mjs');",'')
 .replaceAll('import.meta.url',JSON.stringify(import.meta.url));
const tick=()=>new Promise(resolve=>setImmediate(resolve));
async function fixture(overrides={}) {
 const elements=[];
 class Element {
  constructor(tag){this.tag=tag;this.attrs={};this.listeners={};this.children=[];this.value='';this.disabled=false;this.namespaceURI='http://www.w3.org/2000/svg';elements.push(this)}
  setAttribute(k,v){this.attrs[k]=v;if(k==='disabled')this.disabled=true}
  append(...xs){this.children.push(...xs)}
  replaceChildren(...xs){this.children=xs}
  addEventListener(name,fn){this.listeners[name]=fn}
  dispatch(name,e={}){return this.listeners[name]?.(e)}
  showModal(){} close(){this.dispatch('close')} remove(){} setPointerCapture(){}
  getBoundingClientRect(){return {left:0,top:0,width:1000,height:320}}
 }
 const calls={commit:[],pick:[],configure:[]};
 const ctx={URL,Date,structuredClone,customElements:{get:()=>class {}},document:{createElement:t=>new Element(t),createElementNS:(_,t)=>new Element(t),body:new Element('body')}};
 vm.createContext(ctx);vm.runInContext(code,ctx);
 await ctx.openHandwriting({translate:k=>k,readSettings:async()=>({models:'/models',runtime:'/runtime'}),configure:async(...args)=>calls.configure.push(args),pickPath:async kind=>{calls.pick.push(kind);return null},updateInk:async()=>{},recognize:async()=>({latex:'x+1',complete:true,elapsed_ms:20}),preview:async()=>({mathml:'<math/>'}),commit:async value=>calls.commit.push(value),pending:()=>{},generationValid:()=>true,...overrides});
 const byLabel=label=>elements.find(e=>e.attrs['aria-label']===label);
 return {calls,elements,byLabel,source:byLabel('latex'),add:elements.find(e=>e.textContent==='handwriting_insert'),infer:elements.find(e=>e.textContent==='handwriting_recognize'),svg:elements.find(e=>e.tag==='svg')};
}
async function write(f){f.svg.dispatch('pointerdown',{isPrimary:true,button:0,pointerId:1,clientX:10,clientY:20,preventDefault(){}});f.svg.dispatch('pointermove',{clientX:20,clientY:30});f.svg.dispatch('pointerup');await tick()}
async function correct(f){f.source.value='1+1';f.source.dispatch('input');await tick();assert.equal(f.add.disabled,false)}

test('drawing and undo invalidate a previously accepted candidate',async()=>{
 const f=await fixture();await correct(f);await write(f);assert.equal(f.add.disabled,true);
 await correct(f);f.byLabel('undo').onclick();assert.equal(f.add.disabled,true);
 await f.add.onclick();assert.deepEqual(f.calls.commit,[]);
});
test('a stale preview cannot re-enable insertion after a new stroke',async()=>{
 let resolve;const f=await fixture({preview:()=>new Promise(r=>resolve=r)});
 f.source.value='x';f.source.dispatch('input');await write(f);resolve({mathml:'<math/>'});await tick();assert.equal(f.add.disabled,true);
});
test('native selection cancellation preserves paths; selection requires explicit Apply',async()=>{
 const f=await fixture();const rows=f.elements.filter(e=>e.attrs.class==='ink-path');
 await rows[0].children[1].onclick();assert.equal(rows[0].children[0].value,'/models');assert.deepEqual(f.calls.configure,[]);
 const g=await fixture({pickPath:async()=>'/selected'});const row=g.elements.find(e=>e.attrs.class==='ink-path');await row.children[1].onclick();assert.equal(row.children[0].value,'/selected');assert.deepEqual(g.calls.configure,[]);
});
test('recognition produces a validated editable candidate before explicit commit',async()=>{
 const f=await fixture();await write(f);await f.infer.onclick();assert.equal(f.source.value,'x+1');assert.equal(f.add.disabled,false);assert.deepEqual(f.calls.commit,[]);
 await f.add.onclick();assert.deepEqual(f.calls.commit,['x+1']);
});

test('alternative recognition requires a fresh preview and explicit insertion',async()=>{
 const f=await fixture({recognize:async()=>({latex:'x+1',complete:true,elapsed_ms:20,candidates:[{latex:'x+1',complete:true},{latex:'x-1',complete:true}]})});
 await write(f);await f.infer.onclick();const choices=f.elements.find(e=>e.attrs.class==='ink-candidates');
 assert.equal(choices.children.length,2);await choices.children[1].onclick();
 assert.equal(f.source.value,'x-1');assert.deepEqual(f.calls.commit,[]);
 assert.equal(choices.children[1].attrs['aria-pressed'],'true');
 await f.add.onclick();assert.deepEqual(f.calls.commit,['x-1']);
});
test('a candidate button retained by a stale event cannot restore changed ink',async()=>{
 const f=await fixture({recognize:async()=>({latex:'x',complete:true,elapsed_ms:20,candidates:[{latex:'x',complete:true},{latex:'y',complete:true}]})});
 await write(f);await f.infer.onclick();const choices=f.elements.find(e=>e.attrs.class==='ink-candidates');const stale=choices.children[1];
 await write(f);assert.equal(choices.children.length,0);await stale.onclick();assert.equal(f.add.disabled,true);assert.equal(f.source.value,'x');
});
