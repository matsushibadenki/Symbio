import {referenceKeys} from './math-keyboard-data.js';
import {computationalLatexGroups,referenceNotation} from './math-keyboard-layout.js';
import {editSelection,eraseSelection,DraftHistory,nextPlaceholder} from './math-keyboard-edit.js';
import {t} from './i18n.js';
const reference=(symbol)=>referenceKeys.find(k=>k.s===symbol);
const adapted=(symbol,extra)=>({label:reference(symbol).s,...extra});
const groups={
 basic:[...['7','8','9','4','5','6','1','2','3','0','.','x','y','+'].map(code=>({label:code,code})),adapted('−',{code:'-'}),{label:'×',code:'*'},adapted('/',{code:'/'}),adapted('a⁄b',{template:'fraction'}),adapted('aᵇ',{template:'power'}),adapted('√x',{wrap:'sqrt($)'}),adapted('|x|',{wrap:'abs($)'}),adapted('( )',{wrap:'($)'})],
 functions:['sin','cos','exp','log','sqrt','abs'].map(name=>({label:name,wrap:`${name}($)`})),
 calculus:[adapted('d/dx',{run:'differentiate'}),adapted('∫',{run:'integrate'})]
};
const glyph=k=>((k.c.startsWith('\\left[')?'[ ]':null)||{'分数':'a⁄b','根号':'√x','積分':'∫','極限':'lim'}[k.s]||k.s);
const histories=new Map(),tabs=new Map();
function element(tag,attributes={},text){const e=document.createElement(tag);for(const [k,v]of Object.entries(attributes))e.setAttribute(k,String(v));if(text!==undefined)e.textContent=text;return e;}
export function createMathKeyboard(target,{id,mode='expression',includeReference=false,onRun,onClose}){
 const historyKey=`${id}:${mode}`;
 let history=histories.get(historyKey);
 if(!history||history.entries[history.index]!==target.value)history=new DraftHistory(target.value);
 histories.delete(historyKey);histories.set(historyKey,history);
 while(histories.size>12)histories.delete(histories.keys().next().value);
 const shell=element('section',{class:'math-keyboard','aria-label':t('math_keyboard')});
 const controls=element('div',{class:'keyboard-tabs',role:'group','aria-label':t('keyboard_categories')});
 const grid=element('div',{class:'keyboard-grid',role:'group','aria-label':t('math_keyboard')});
 const message=element('span',{class:'keyboard-message small',role:'status'});
 const nav=element('div',{class:'keyboard-nav'});
 const categories=mode==='latex'?{...computationalLatexGroups,...(includeReference?{reference:referenceNotation}:{})}:groups;
 let tab=tabs.get(historyKey)||'basic';if(!categories[tab])tab='basic';
 const keyButton=(label,title,fn,attrs={})=>{const b=element('button',{type:'button',title,'aria-label':title,...attrs},label);b.addEventListener('pointerdown',e=>{if(e.button===0)e.preventDefault()});b.addEventListener('click',fn);return b};
 function apply(edit,record=true){
  if(new TextEncoder().encode(edit.value).length>Math.min(target.maxLength>0?target.maxLength:4096,mode==='latex'?4096:2048)){message.textContent=t('resource_limit');return}
  target.value=edit.value;if(record)history.record(edit.value);
  target.dispatchEvent(new Event('input',{bubbles:true}));target.focus();target.setSelectionRange(edit.start,edit.end);message.textContent='';refreshHistory();
 }
 function move(delta){const value=history.move(delta);apply({value,start:value.length,end:value.length},false)}
 const undo=keyButton('↶',t('keyboard_undo'),()=>move(-1));
 const redo=keyButton('↷',t('keyboard_redo'),()=>move(1));
 function refreshHistory(){undo.disabled=history.index===0;redo.disabled=history.index===history.entries.length-1}
 target.addEventListener('input',()=>{history.record(target.value);refreshHistory()});
 function placeholder(backward=false){const range=nextPlaceholder(target.value,target.selectionStart,target.selectionEnd,backward);if(range){target.focus();target.setSelectionRange(range.start,range.end)}return range}
 target.addEventListener('keydown',e=>{if(e.key==='Tab'&&placeholder(e.shiftKey)){e.preventDefault();e.stopPropagation();return;}if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='z'){e.preventDefault();e.stopPropagation();move(e.shiftKey?1:-1)}});
 for(const key of Object.keys(categories)){
  const b=keyButton(t(`keyboard_${key}`),t(`keyboard_${key}`),()=>{tab=key;tabs.set(historyKey,key);draw()},{'data-category':key});controls.append(b);
 }
 controls.append(element('span',{class:'spacer'}),element('span',{class:'small'},mode==='latex'?'LaTeX':t('source')),keyButton('×',t('close'),onClose));
 function draw(){grid.replaceChildren();controls.querySelectorAll('[data-category]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.category===tab)));
  const keys=mode==='latex'?categories[tab].map(k=>({label:glyph(k),code:k.c})):categories[tab];
  for(const key of keys){
   const title=key.run?`${t(key.run)} · ${t('keyboard_whole_cell')}`:key.template==='fraction'?t('keyboard_fraction'):key.template==='power'?t('keyboard_power'):key.wrap?.replace('$','x')||key.code||key.label;
   grid.append(keyButton(key.label,title,()=>{if(key.run){onRun?.(key.run);return}apply(editSelection(target.value,target.selectionStart,target.selectionEnd,key,mode))}));
  }
 }
 nav.append(undo,redo,keyButton('□ →',t('next_placeholder'),()=>placeholder()),
  keyButton('←',t('keyboard_left'),()=>{const p=target.selectionStart!==target.selectionEnd?target.selectionStart:Math.max(0,target.selectionStart-1);target.focus();target.setSelectionRange(p,p)}),
  keyButton('→',t('keyboard_right'),()=>{const p=target.selectionStart!==target.selectionEnd?target.selectionEnd:Math.min(target.value.length,target.selectionEnd+1);target.focus();target.setSelectionRange(p,p)}),
  keyButton('⌫',t('keyboard_backspace'),()=>apply(eraseSelection(target.value,target.selectionStart,target.selectionEnd,true))),
  keyButton('⌦',t('keyboard_delete'),()=>apply(eraseSelection(target.value,target.selectionStart,target.selectionEnd,false))),
  keyButton('␣',t('keyboard_space'),()=>apply(editSelection(target.value,target.selectionStart,target.selectionEnd,{code:' '},mode))),message);
 shell.append(controls,grid,nav);draw();refreshHistory();return shell;
}
