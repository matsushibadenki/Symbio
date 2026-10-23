import test from 'node:test';
import assert from 'node:assert/strict';
import {bindLatexPreview} from '../apps/desktop/ui/latex-preview.js';
const tick=()=>new Promise(resolve=>setTimeout(resolve,200));
function fixture(){
 const target=new EventTarget();target.value='x';
 const host={isConnected:true,dataset:{},children:[],textContent:'',replaceChildren(...children){this.children=children;this.textContent=''},removeAttribute(name){delete this[name]}};
 const requests=[];
 bindLatexPreview(target,host,{request:source=>new Promise((resolve,reject)=>requests.push({source,resolve,reject})),translate:key=>key,renderFormula:mathml=>({mathml})});
 const input=value=>{target.value=value;target.dispatchEvent(new Event('input'))};
 return {target,host,requests,input};
}
test('a late response never replaces the newest LaTeX preview',async()=>{
 const {host,requests,input}=fixture();await tick();input('x^2');await tick();
 requests[1].resolve({mathml:'new',expression:'(x^2)'});await Promise.resolve();
 requests[0].resolve({mathml:'old',expression:'x'});await Promise.resolve();
 assert.deepEqual(host.children,[{mathml:'new'}]);assert.equal(host.title,'(x^2)');
});
test('invalid input clears the previous formula and reports an error',async()=>{
 const {host,requests,input}=fixture();await tick();requests[0].resolve({mathml:'valid',expression:'x'});await Promise.resolve();
 input('\\frac{');assert.equal(host.children.length,0);await tick();requests[1].reject({code:'parse_error',detail:'token 2'});await Promise.resolve();
 assert.equal(host.dataset.state,'error');assert.equal(host.textContent,'parse_error token 2');assert.equal(host.title,undefined);
});
test('detached panels and empty inputs do not request a preview',async()=>{
 const {host,requests,input}=fixture();input('');await tick();assert.equal(requests.length,0);
 input('x');host.isConnected=false;await tick();assert.equal(requests.length,0);
});
