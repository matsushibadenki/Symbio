import {compareDocumentSnapshot} from './document-state.js';
import {t,setLocale,locale} from './i18n.js';
import {icon} from './icons.js';
import {definitionOf,definitionChanged} from './graph-editor-state.js';
import {renderSurfaceAxes} from './surface-axes.js';
import {dependencyLayout} from './dependency-layout.js';
import {bindLatexPreview} from './latex-preview.js';
import {createMathKeyboard} from './math-keyboard.js';
import {bindDock,renderTabs,bindSplitter} from './dock.js';
const api=window.__TAURI__;const view=new URLSearchParams(location.search).get('view')||'preview';
const root=document.getElementById('app');let current=null,layout=null,busy=false,statusKey='ready',detail='',inspection=null,inspectionRevision=-1;
let surface=null,surfaceAxes=null;
let graphEditing=null;
const expandedEditors=new Set(),expandedSections=new Set();let revealCell=null;
let dependencyZoom=view==='dependencies'?1.6:1;
let keyboard=null;const notePreviews=new Set();
const drafts=new Map();let draftQueue=Promise.resolve();
function pendingDrafts(){return current?.cells.some(c=>["name","source","source_format","note"].some(k=>drafts.get(c.id)?.[k]!==undefined&&drafts.get(c.id)[k]!==c[k]))||false}
function publishDrafts(){if(api){const pending=pendingDrafts();draftQueue=draftQueue.catch(()=>{}).then(()=>invoke("set_draft",{pending}));}return draftQueue}
async function flushDrafts(){for(const c of [...current.cells]){await commit(c);const note=drafts.get(c.id)?.note;if(note!==undefined&&note!==c.note){await mutate({method:"math.note",id:c.id,text:note});delete drafts.get(c.id).note;}}await publishDrafts();}
export function el(tag,attrs={},...children){const n=document.createElement(tag);for(const [key,val]of Object.entries(attrs)){if(key==='class')n.className=val;else if(key.startsWith('on'))n.addEventListener(key.slice(2),val);else if(key==='text')n.textContent=val;else if(key==='value')n.value=val;else if(key==='checked')n.checked=val;else if(val!==null)n.setAttribute(key,String(val));}for(const c of children.flat()){if(c!==null&&c!==undefined)n.append(typeof c==='string'?document.createTextNode(c):c)}return n}
export function button(key,shape,fn,options={}){return el('button',{type:'button',title:t(key),'aria-label':t(key),onclick:fn,...options},shape?icon(shape):t(key))}
const label=(key,node)=>el('label',{},el('span',{class:'small',text:t(key)}),node);
function field(key,value,attrs={}){return el('input',{'aria-label':t(key),title:t(key),value,...attrs})}
function formula(html){
 const n=el('div',{class:'formula'}),body=el('div',{class:'math-scroll'});body.innerHTML=html; // Only Rust-generated MathML.
 for(const ref of body.querySelectorAll('[data-reference]')){ref.setAttribute('tabindex','0');ref.setAttribute('role','link');ref.setAttribute('aria-label',t('reference_to')+' '+ref.dataset.reference);ref.setAttribute('title',t('reference_to')+' '+ref.dataset.reference);}
 const zoom=button('formula_focus','float',()=>showFormula(html),{class:'formula-zoom'});
 n.append(body,zoom);return n;
}
function showFormula(html){
 const body=el('div',{class:'formula-stage'});body.innerHTML=html;
 const size=el('input',{type:'range',min:22,max:72,value:44,'aria-label':t('formula_size'),oninput:e=>body.style.setProperty('--formula-size',e.target.value+'px')});
 const dialog=el('dialog',{class:'formula-dialog'},el('div',{class:'dialog-head'},el('strong',{text:t('formula_focus')}),size,button('close','close',()=>dialog.close())),body);
 dialog.addEventListener('close',()=>dialog.remove());document.body.append(dialog);dialog.showModal();
}

export async function invoke(name,args){if(current&&['set_draft','preview_latex','inspect','show_plot'].includes(name))args={...args,generation:current.generation};return api.core.invoke(name,args)}
export function getLayout(){return layout}export function getView(){return view}
function status(key,errorDetail=''){statusKey=key;detail=errorDetail;document.querySelectorAll('.status,.surface-status').forEach(n=>{n.textContent=t(key)+(errorDetail?` ${errorDetail}`:'');n.dataset.error=String(!['ready','working'].includes(key));});}
export async function action(fn){if(busy||!api)return;busy=true;document.querySelectorAll('button').forEach(b=>b.disabled=true);status('working');try{await draftQueue;await fn();status('ready')}catch(e){status(e.code||'error',e.detail||e.message||'');if(['revision_conflict','document_changed'].includes(e.code))accept(await invoke('snapshot'));if(['layout_conflict','panel_error','layout_save_failed'].includes(e.code))applyLayout(await invoke('workspace_state'));}finally{busy=false;render();}}
async function mutate(command){const s=await invoke('edit',{generation:current.generation,request:{expected_revision:current.revision,actor:'human',command}});accept(s);return s}
export function dockCommand(command){return action(async()=>applyLayout(await invoke('workspace_command',{command,expectedRevision:layout.revision})))}
function accept(s){const decision=compareDocumentSnapshot(current,s);if(decision==='ignore')return;if(current&&decision!=='replace'&&current.active===s.active&&s.cells.length>current.cells.length&&!current.cells.some(c=>c.id===s.selected))expandedEditors.add(`${s.active_document??s.project_id}:${s.active}:editor-${s.selected}`);if(current?.selected!==s.selected||decision==='replace')revealCell=s.selected;if(decision==='replace'){drafts.clear();notePreviews.clear();expandedEditors.clear();graphEditing=null;keyboard=null;inspection=null;inspectionRevision=-1;document.querySelectorAll('dialog').forEach(d=>d.close());}current=s;if(statusKey==='unsaved_changes'&&!s.dirty&&!pendingDrafts()){statusKey='ready';detail=''}if(!busy)render();refreshInspection();}
async function fileAction(command){await flushDrafts();if(command==='save'&&!current.path)command='save_as';const s=await invoke(command,{generation:current.generation});if(s)accept(s);}

export function applyLayout(s){if(layout&&s.revision<=layout.revision)return;layout=s;setLocale(s.locale);if(view==='notebook')revealCell=current?.selected;if(!busy)render();}
async function refreshInspection(){if(!api||!current?.selected||view!=='inspector'||inspectionRevision===current.revision)return;const revision=current.revision,generation=current.generation;inspectionRevision=revision;try{const data=await invoke('inspect',{id:current.selected});if(current.revision===revision&&current.generation===generation){inspection=data;if(!busy)render();}}catch(e){inspection=null;status(e.code||'error')}}
function remember(id,part,value){const d=drafts.get(id)||{};d[part]=value;drafts.set(id,d);if(["name","source","source_format","note"].includes(part))publishDrafts()}
function valueFor(c,part){return drafts.get(c.id)?.[part]??c[part]??(part==='source_format'?'expression':undefined)}
async function commit(c){if(definitionChanged(drafts.get(c.id)?.graphBase,c))throw {code:'graph_edit_conflict'};const name=valueFor(c,'name'),source=valueFor(c,'source'),format=valueFor(c,'source_format');if(name!==c.name||source!==c.source||format!==(c.source_format||'expression')){await mutate({method:format==='latex'?'math.define_latex':'math.define',id:c.id,name,source});const d=drafts.get(c.id);if(d){delete d.name;delete d.source;delete d.source_format;delete d.graphBase;}await publishDrafts();}}
function latexPreview(target,onResult,id=null){const host=el('div',{class:'latex-preview','aria-label':t('latex_preview'),role:'status','aria-live':'polite'});if(api)bindLatexPreview(target,host,{request:source=>invoke('preview_latex',{source,id}),translate:t,renderFormula:formula,onResult});return host;}

function header(panel){const tabs=renderTabs(panel);const menu=el('details',{class:'panel-menu'},el('summary',{title:t('dock'),'aria-label':t('dock')},icon('dock')));const target=el('select',{'aria-label':t('dock')},...['objects','notebook','surface','inspector'].filter(p=>p!==panel).map(p=>el('option',{value:p,text:t(p)})));const zone=el('select',{'aria-label':t('dock')},...['center','left','right','top','bottom'].map(p=>el('option',{value:p,text:t(p)})));menu.append(el('div',{},target,zone,button('apply','check',()=>dockCommand({kind:'dock',panel,target:target.value,zone:zone.value})),button('float_group','float',()=>dockCommand({kind:'float_group',panel})),button('return_group','dock',()=>dockCommand({kind:'return_group',panel}))));const floating=layout?.floating.includes(panel);return el('div',{class:'panel-header'},tabs,button(floating?'return_panel':'float',floating?'dock':'float',()=>dockCommand(floating?{kind:'return',panel}:{kind:'dock',panel,target:null,zone:'floating'})),menu)}
function footer(){return el('div',{class:'status','role':'status','aria-live':'polite','data-error':!['ready','working'].includes(statusKey)},el('span',{text:t(statusKey)+(detail?` ${detail}`:'')}),statusKey==='ready'?el('span',{class:'spacer'}):null,statusKey==='ready'?el('span',{class:'quiet',text:`${current?.active||'main'} · r${current?.revision||0}`}):null)}
function documentPicker(){
 if(!current?.documents)return null;
 const picker=el('select',{'aria-label':t('open_documents'),title:t('open_documents'),onchange:e=>action(async()=>{await flushDrafts();accept(await invoke('switch_document',{generation:current.generation,id:Number(e.target.value)}))})},...current.documents.map(d=>el('option',{value:d.id,title:d.path||t('untitled'),text:`${d.id} · ${d.name}${d.dirty?' ●':!d.path?' ○':''}`})));
 picker.value=current.active_document;
 return el('div',{class:'document-picker'},icon('documents'),picker,button('close_document','close',()=>action(()=>fileAction('close_document'))));
}
function toolbar(){const title=field('name',current?.name||'Symbio',{class:'title-input',onchange:e=>action(()=>mutate({method:'math.rename',name:e.target.value}))});const languages=el('select',{'aria-label':t('locale'),onchange:e=>dockCommand({kind:'locale',locale:e.target.value})},el('option',{value:'en',text:'EN'}),el('option',{value:'ja',text:'日本語'}),el('option',{value:'zh-CN',text:'简体中文'}));languages.value=locale;return el('div',{class:'toolbar'},el('div',{class:'brand'},el('span',{class:'brand-mark',text:'∿'}),'Symbio'),title,documentPicker(),el('span',{class:'spacer'}),view==='preview'?el('span',{class:'preview-banner',text:t('preview')}):null,button('undo','undo',()=>action(()=>mutate({method:'math.undo'})),current?.can_undo?{}:{disabled:''}),button('redo','redo',()=>action(()=>mutate({method:'math.redo'})),current?.can_redo?{}:{disabled:''}),el('span',{class:'separator'}),button('new_document','new',()=>action(()=>fileAction('new_document'))),button('open_document','open',()=>action(()=>fileAction('open_document'))),button('save','save',()=>action(()=>fileAction('save'))),button('save_as','saveAs',()=>action(()=>fileAction('save_as'))),el('span',{class:'small toolbar-status',role:'status',title:detail||current?.path||t('untitled'),text:['ready','working'].includes(statusKey)?t(current?.dirty||!current?.path?'unsaved':'saved'):t(statusKey)}),button('export','json',showJson),button('reset_layout','reset',()=>dockCommand({kind:'reset'})),languages)}
function objects(){
 const add=()=>action(()=>mutate({method:'math.define_latex',id:null,name:`e${Math.max(0,...current.cells.map(c=>c.id))+1}`,source:'x^{2}'}));
 const out=el('div',{class:'content side-content'},el('div',{class:'section-title'},el('h2',{text:t('expressions')}),button('add','add',add)));
 out.append(el('ul',{class:'object-list'},...current.cells.map(c=>{const preview=el('div',{class:'object-formula'});preview.innerHTML=c.mathml;const parameters=c.parameters??c.symbols;const signature=c.dependent_symbol?`${parameters.join(', ')||'—'} → ${c.dependent_symbol}`:parameters.length?parameters.join(', '):c.dependencies?.length?`${t('references')} · ${c.dependencies.length}`:t('constant_expression');return el('li',{},el('button',{class:'object-row','aria-pressed':current.selected===c.id,onclick:()=>action(()=>selectCell(c.id))},el('span',{class:'symbol',text:c.name}),el('span',{class:'object-meta'},el('span',{text:signature}),preview)))})));
 out.append(el('div',{class:'section-title'},el('h2',{text:t('assumptions')}),button('unassigned_domain_help','info',()=>{})));
 const symbols=[...new Set([...Object.keys(current.assumptions),...current.cells.flatMap(c=>c.parameters??c.symbols)])].sort();for(const symbol of symbols)out.append(assumptionForm(symbol,current.assumptions[symbol]??{domain:'unspecified',nonzero:false,dimension:null},!current.assumptions[symbol]));
 const symbol=field('symbol','',{placeholder:'x',maxlength:48});out.append(el('form',{class:'branch-row',onsubmit:e=>{e.preventDefault();action(()=>mutate({method:'math.assume',symbol:symbol.value,assumption:{domain:'unspecified',nonzero:false,dimension:null}}))}},symbol,button('add','add',()=>symbol.form.requestSubmit())));
 out.append(el('div',{class:'section-title'},el('h2',{text:t('branch')}),icon('branch')));
 const branches=el('select',{'aria-label':t('branch'),onchange:e=>action(()=>mutate({method:'math.checkout',name:e.target.value}))},...current.branches.map(b=>el('option',{value:b,text:b})));branches.value=current.active;out.append(el('div',{class:'branch-row'},branches));
 const branch=field('new_branch','',{placeholder:t('new_branch'),maxlength:64});out.append(el('form',{class:'branch-row',onsubmit:e=>{e.preventDefault();action(()=>mutate({method:'math.branch',name:branch.value}))}},branch,button('create_branch','branch',()=>branch.form.requestSubmit())));
 return out;
}
function assumptionForm(symbol,a,inferred=false){
 const domains=el('select',{'aria-label':`${symbol} · ${t('assumptions')}`},...['unspecified','real','nonnegative','positive','complex'].map(d=>el('option',{value:d,text:t(d)})));domains.value=a.domain;const nonzero=el('input',{type:'checkbox',checked:a.nonzero});const dimensions=[0,1,2].map(i=>field('dimension',a.dimension?.[i]??'',{type:'number',min:-16,max:16,placeholder:['M','L','T'][i]}));
 const form=el('form',{class:'assumption',title:inferred?t('unassigned_domain_help'):'',onsubmit:e=>{e.preventDefault();const dimension=dimensions.every(d=>d.value==='')?null:dimensions.map(d=>Number(d.value));action(()=>mutate({method:'math.assume',symbol,assumption:{domain:domains.value,nonzero:nonzero.checked,dimension}}))}},el('div',{class:'assumption-top'},el('span',{class:'symbol',text:symbol}),domains,button('apply','check',()=>form.requestSubmit())),el('label',{},nonzero,t('nonzero')),el('div',{class:'dimension',title:t('dimension')},...dimensions));return form;
}
function sectionKey(name){return `${current.active_document??current.project_id}:${current.active}:${name}`}
function disclosure(name,title,...children){const key=sectionKey(name);const details=el('details',{class:'inspector-disclosure',...(expandedSections.has(key)?{open:''}:{})},el('summary',{},title),...children);details.addEventListener('toggle',()=>{if(details.open)expandedSections.add(key);else expandedSections.delete(key)});return details;}
async function selectCell(id){await mutate({method:'math.select',id});revealCell=id;if(['objects','inspector','dependencies'].includes(view)&&layout)applyLayout(await invoke('reveal_notebook',{generation:current.generation}));}
function notebook(){const out=el('div',{class:'content notebook'});if(!current.cells.length)out.append(el('div',{class:'empty',text:t('empty')}));for(const c of current.cells)out.append(cell(c));return out}
function toggleKeyboard(c,kind,target){
 const start=target.selectionStart,end=target.selectionEnd;
 keyboard=keyboard?.id===c.id&&keyboard.kind===kind?null:{id:c.id,kind};render();
 const next=root.querySelector(`[data-focus="${kind}-${c.id}"]`);if(next){next.focus();next.setSelectionRange(start,end)}
}
function keyboardButton(c,kind,target){return button(kind==='note'?'keyboard_latex':'math_keyboard','keyboard',()=>toggleKeyboard(c,kind,target),{'aria-expanded':keyboard?.id===c.id&&keyboard.kind===kind,onpointerdown:e=>e.preventDefault()})}
function cell(c){
 const editorKey=sectionKey(`editor-${c.id}`),selected=current.selected===c.id;
 const draftChanged=['name','source','source_format'].some(k=>valueFor(c,k)!==(c[k]??(k==='source_format'?'expression':undefined)));
 const editing=expandedEditors.has(editorKey)||draftChanged||(keyboard?.id===c.id&&keyboard.kind==='source');
 const finishEdit=()=>{expandedEditors.delete(editorKey);keyboard=null;};
 const discardSource=()=>action(async()=>{const d=drafts.get(c.id);if(d)for(const key of ['name','source','source_format','graphBase'])delete d[key];finishEdit();await publishDrafts();});
 const name=field('name',valueFor(c,'name'),{class:'cell-name',maxlength:48,'data-focus':`name-${c.id}`,oninput:e=>remember(c.id,'name',e.target.value)});
 const source=el('textarea',{'aria-label':`${t('source')} ${c.id}`,class:'cell-source',spellcheck:'false',autocorrect:'off',autocapitalize:'off',rows:2,maxlength:2048,'data-focus':`source-${c.id}`,oninput:e=>{remember(c.id,'source',e.target.value);refreshDraft()}});source.value=valueFor(c,'source');
 const format=valueFor(c,'source_format');
 const sourceFormat=el('select',{'aria-label':t('source_format'),title:t('source_format'),onchange:e=>{const target=e.target.value;action(async()=>{const converted=await invoke('convert_source',{source:valueFor(c,'source'),fromFormat:format,toFormat:target});remember(c.id,'source',converted);remember(c.id,'source_format',target);await publishDrafts()})}},el('option',{value:'expression',text:t('expression_format')}),el('option',{value:'latex',text:'LaTeX'}));sourceFormat.value=format;
 const segLabel=el('span',{class:'small'});
 function refreshDraft(){const changed=valueFor(c,'source')!==c.source||format!==(c.source_format||'expression');form.dataset.draft=String(changed);segLabel.textContent=changed?t('draft_preview'):t('source_saved');}
 let inputSymbols=c.parameters??c.symbols??[];
 const exactValues={...(c.calculation?.exact_bindings||{}),...(drafts.get(c.id)?.exactBindings||{})};
 const bindingValues={...(c.calculation?.operation==='evaluate'?c.calculation.bindings:{}),...(drafts.get(c.id)?.numericBindings||{})};
 const variable=field('variable',drafts.get(c.id)?.variable??c.calculation?.variable??'x',{class:'variable',spellcheck:'false',autocorrect:'off',autocapitalize:'off',oninput:e=>{remember(c.id,'variable',e.target.value);updateOperationLabels()}});
 const run=operation=>action(async()=>{if(operation==='substitute'){const values=Object.fromEntries([...exactInputs.querySelectorAll('input')].filter(n=>n.value.trim()).map(n=>[n.dataset.symbol,n.value.trim()]));if(!Object.keys(values).length)throw {code:'exact_bindings_required'};await commit(c);await mutate({method:'math.substitute',id:c.id,values});finishEdit();return;}const values={};if(operation==='evaluate'){for(const input of numericInputs.querySelectorAll('input')){if(input.value.trim()===''||!input.validity.valid||!Number.isFinite(input.valueAsNumber))throw {code:'numeric_bindings_required'};values[input.dataset.symbol]=input.valueAsNumber;}}await commit(c);await mutate({method:'math.transform',id:c.id,operation,variable:variable.value,bindings:values});finishEdit();});
 const contextual=el('span',{class:'context-operations'});let cellKind=c.kind||'scalar';
 function updateKind(result){const kind=result.kind||c.kind||'scalar';cellKind=kind;if(contextual.parentElement)contextual.parentElement.dataset.kind=kind;contextual.replaceChildren();
  const operations=kind==='matrix'?{determinant:'det',inverse:'A⁻¹',transpose:'Aᵀ'}:kind==='equation'?{solve:'x = ?'}:{};
  for(const [operation,glyph]of Object.entries(operations))contextual.append(el('button',{type:'button',title:t(operation),'aria-label':`${t(operation)} ${c.id}`,'data-operation':operation,onclick:()=>run(operation)},glyph));
 }
 updateKind({kind:c.kind});
 const ops=el('div',{class:'operation-bar'},el('button',{type:'button',class:'primary',title:t('compute_help'),'aria-label':`${t('compute')} ${c.id}`,onclick:()=>run('compute')},el('span',{class:'word',text:t('compute')})),contextual,...Object.entries({differentiate:'d/dx',integrate:'∫',expand:'( )',evaluate:'≈'}).map(([key,glyph])=>el('button',{type:'button',title:t(key),'data-operation':key,'aria-label':`${t(key)} ${c.id}`,onclick:()=>{if(key==='evaluate'){numericSettings.open=true;numericInputs.querySelector('input')?.focus();return}run(key)}},key==='evaluate'?el('span',{class:'word',text:'≈ '+t('numeric_short')}):glyph)),el('button',{type:'button','data-operation':'substitute',class:'word',title:t('substitute_help'),'aria-label':`${t('substitute')} ${c.id}`,onclick:()=>{exactSettings.open=true;exactInputs.querySelector('input')?.focus()}},t('substitute')),button('surface_show','surface',()=>action(async()=>{await commit(c);surface=await invoke('surface_command',{command:{kind:'select',id:c.id}})})),button('plot','plot',()=>action(async()=>{await commit(c);await invoke('show_plot',{id:c.id})}),{'data-operation':'plot'}),label('variable',variable),el('span',{class:'spacer'}),button('info','info',()=>{source.focus()}));
 ops.dataset.kind=cellKind;
 ops.insertBefore(ops.querySelector('[data-operation="substitute"]'),ops.querySelector('[data-operation="evaluate"]'));
 const numericInputs=el('div',{class:'numeric-inputs'});
 const numericSettings=el('details',{class:'numeric-settings'},el('summary',{},icon('settings'),el('span',{text:t('numeric_parameters')})),numericInputs,el('button',{type:'button',class:'outline',title:t('evaluate'),'aria-label':`${t('evaluate')} ${c.id}`,onclick:()=>run('evaluate')},'≈',el('span',{class:'word',text:t('evaluate')})));
 function refreshNumericBindings(){numericInputs.replaceChildren(...inputSymbols.map(symbol=>el('label',{class:'numeric-binding'},el('span',{text:symbol}),el('input',{type:'number',step:'any','aria-required':'true',value:bindingValues[symbol]??'','data-symbol':symbol,'aria-label':`${t('numeric_value')} ${symbol}`,oninput:e=>{bindingValues[symbol]=e.target.value;remember(c.id,'numericBindings',{...bindingValues})}}))));if(!inputSymbols.length)numericInputs.append(el('span',{class:'small',text:t('no_numeric_parameters')}));}
 refreshNumericBindings();
 const exactInputs=el('div',{class:'numeric-inputs'});
 const exactSettings=el('details',{class:'numeric-settings exact-settings'},el('summary',{},icon('settings'),el('span',{text:t('exact_parameters')})),exactInputs,button('exact_values_help','info',()=>{}),el('button',{type:'button',class:'outline',title:t('substitute'),'aria-label':`${t('apply_substitution')} ${c.id}`,onclick:()=>run('substitute')},t('apply_substitution')));
 function refreshExactBindings(){exactInputs.replaceChildren(...inputSymbols.map(symbol=>el('label',{class:'numeric-binding'},el('span',{text:symbol}),el('input',{type:'text',value:exactValues[symbol]??'',placeholder:'2, 1/3','data-symbol':symbol,'aria-label':`${t('exact_value')} ${symbol}`,spellcheck:'false',autocorrect:'off',autocapitalize:'off',oninput:e=>{exactValues[symbol]=e.target.value;remember(c.id,'exactBindings',{...exactValues})}}))));if(!inputSymbols.length)exactInputs.append(el('span',{class:'small',text:t('no_numeric_parameters')}));}
 refreshExactBindings();
 const form=el('form',{class:'input-form notebook-cell-body',novalidate:'','data-editing':editing,onsubmit:e=>{e.preventDefault();action(async()=>{await commit(c);finishEdit()})}},el('div',{class:'authored-formula equation-row'},el('span',{class:'equation-label',text:t('definition')}),formula(c.definition_mathml||c.mathml)));
 if(editing){
  const preview=el('div',{class:'latex-preview input-render'});
  if(api)bindLatexPreview(source,preview,{request:async text=>{const latex=format==='latex'?text:await invoke('convert_source',{source:text,fromFormat:'expression',toFormat:'latex'});return invoke('preview_latex',{source:latex,id:c.id})},translate:t,renderFormula:formula,onResult:result=>{inputSymbols=result.parameters??result.symbols??inputSymbols;updateKind(result);updateOperationLabels();refreshNumericBindings();refreshExactBindings()}});
  else preview.append(formula(c.mathml));
  form.append(el('div',{class:'source-editor'},source,el('div',{class:'cell-top'},button('apply','check',()=>form.requestSubmit()),button('discard_edit','undo',discardSource),keyboardButton(c,'source',source),sourceFormat,button('latex_help','info',()=>source.focus()),button('reference_help','branch',()=>source.focus()),segLabel),preview));
  if(keyboard?.id===c.id&&keyboard.kind==='source')form.querySelector('.source-editor').append(createMathKeyboard(source,{id:`${current.active}:${c.id}`,mode:format,onRun:run,onClose:()=>{keyboard=null;render()}}));
 }
 form.append(ops,exactSettings,numericSettings);
 function updateOperationLabels(){const symbol=variable.value.trim()||'x';for(const [key,text]of Object.entries({differentiate:`${inputSymbols.some(s=>s!==symbol)?'∂/∂':'d/d'}${symbol}`,integrate:`∫ d${symbol}`,solve:`${symbol} = ?`})) {const control=ops.querySelector(`[data-operation="${key}"]`);if(control)control.textContent=text;}}
 updateOperationLabels();

 if(c.dependencies?.length){const links=el('div',{class:'cell-dependencies'},el('span',{class:'small',text:t('references')}));for(const id of c.dependencies){const dependency=current.cells.find(c=>c.id===id);links.append(el('button',{type:'button',class:'reference-chip',onclick:()=>action(()=>selectCell(id)),text:dependency.name}));}form.append(links);form.append(el('details',{class:'resolved-definition'},el('summary',{text:t('resolved_definition')}),formula(c.resolved_mathml)));}
 if(c.calculation_error){form.append(el('div',{class:'calculation-error',role:'alert'},el('strong',{text:t('recalculation_failed')}),el('span',{text:t(c.calculation_error)})));}
 if(c.output){form.append(el('div',{class:'output'},el('div',{class:'output-header'},el('span',{text:t(c.output.operation)}),el('span',{class:`trust-badge ${c.output.trust}`,text:t(c.output.trust)})),el('div',{class:'equation-row'},el('span',{class:'equation-label',text:t('result')}),c.result_mathml?formula(c.result_mathml):el('div',{class:'numeric',text:c.result_text})),el('div',{class:'result-metadata'},el('span',{class:'conditions',text:c.output.conditions.map(conditionLabel).join(' · ')}),el('span',{class:'small',text:c.output.precision==='exact'?t('exact'):c.output.precision==='float64; no certified error bound'?t('float64'):c.output.precision}))));}else if(!c.calculation_error)form.append(el('div',{class:'small no-result',text:t('no_result')}));

 const note=el('textarea',{'aria-label':`${t('note')} ${c.id}`,rows:2,maxlength:4096,'data-focus':`note-${c.id}`,oninput:e=>remember(c.id,'note',e.target.value)});note.value=valueFor(c,'note');
 const noteKey=`${current.active}:${c.id}`;
 const showNotePreview=notePreviews.has(noteKey)||(keyboard?.id===c.id&&keyboard.kind==='note');
 const noteTools=el('div',{class:'note-tools'},keyboardButton(c,'note',note),button('latex_preview','plot',()=>{if(notePreviews.has(noteKey))notePreviews.delete(noteKey);else notePreviews.add(noteKey);render()},{'aria-pressed':showNotePreview}),button('note_to_seg','add',()=>action(async()=>{
  await invoke('preview_latex',{source:note.value});
  let n=current.cells.length+1;while(current.cells.some(cell=>cell.name===`latex${n}`))n++;
  await mutate({method:'math.define_latex',id:null,name:`latex${n}`,source:note.value});
 })),button('keyboard_note_help','info',()=>note.focus()));
 const noteKeyboard=keyboard?.id===c.id&&keyboard.kind==='note'?createMathKeyboard(note,{id:noteKey,mode:'latex',includeReference:true,onClose:()=>{keyboard=null;render()}}):null;
 const noteDisclosureKey=sectionKey(`note-${c.id}`);const noteSection=el('details',{class:'cell-note',...((showNotePreview||expandedSections.has(noteDisclosureKey))?{open:''}:{})},el('summary',{text:t('note')}),noteTools,note,showNotePreview?latexPreview(note):null,noteKeyboard,button('save_note','check',()=>action(async()=>{await mutate({method:'math.note',id:c.id,text:note.value});const d=drafts.get(c.id);if(d)delete d.note;await publishDrafts();})));noteSection.addEventListener('toggle',()=>{if(noteSection.open)expandedSections.add(noteDisclosureKey);else expandedSections.delete(noteDisclosureKey)});form.append(noteSection);
 if(c.calculated_revision!=null)form.append(el('div',{class:'small calculation-revision',text:`${t('recalculated')} · r${c.calculated_revision}`}));
 refreshDraft();
 const edit=button('edit_cell','edit',()=>{if(editing&&draftChanged){status('draft_pending');return}if(editing)expandedEditors.delete(editorKey);else expandedEditors.add(editorKey);render();if(!editing)root.querySelector(`[data-focus="source-${c.id}"]`)?.focus();},{'aria-expanded':editing});
 return el('article',{class:'cell','data-cell':c.id,'data-selected':selected},el('div',{class:'cell-top'},el('button',{type:'button',class:'cell-number','aria-label':`${t('select_cell')} ${c.name}`,'aria-pressed':selected,onclick:()=>action(()=>selectCell(c.id)),text:String(c.id).padStart(2,'0')}),name,el('span',{class:'spacer'}),edit,button('remove','remove',()=>action(()=>mutate({method:'math.remove',id:c.id})))),form);
}
function dependencyGraph(){
 const graph=dependencyLayout(current.cells,current.dependencies||[]),stage=el('div',{class:'dependency-stage'}),scale=view==='dependencies'?dependencyZoom:Math.min(1,Math.max(160,root.clientWidth-36)/graph.width);stage.style.width=graph.width*scale+'px';stage.style.height=graph.height*scale+'px';stage.style.setProperty('--graph-scale',String(scale));
 const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox',`0 0 ${graph.width} ${graph.height}`);svg.setAttribute('aria-hidden','true');svg.classList.add('dependency-edges');
 for(const edge of graph.edges){const path=document.createElementNS(svg.namespaceURI,'path');path.setAttribute('d',edge.path);if(edge.to===current.selected||edge.from===current.selected)path.classList.add('active');svg.append(path);const arrow=document.createElementNS(svg.namespaceURI,'path');arrow.setAttribute('d',edge.arrow);svg.append(arrow)}stage.append(svg);
 for(const node of graph.nodes){const c=node.cell;const nodeButton=el('button',{type:'button',class:'dependency-cell','aria-pressed':current.selected===c.id,'data-error':Boolean(c.calculation_error),title:c.expression,onclick:()=>action(async()=>{if(graphEditing!==c.id&&pendingDrafts())throw {code:'draft_pending'};graphEditing=c.id;await mutate({method:'math.select',id:c.id})})},el('strong',{text:c.name}),el('small',{text:c.calculation_error?t('recalculation_failed'):c.calculation?t(c.calculation.operation):t('source')}));nodeButton.style.left=node.x*scale+'px';nodeButton.style.top=node.y*scale+'px';stage.append(nodeButton);}
 return el('div',{class:'dependency-scroll','aria-label':t('dependency_graph')},stage);
}
function graphRemember(c,key,value){const d=drafts.get(c.id)||{};if(!d.graphBase)d.graphBase=definitionOf(c);drafts.set(c.id,d);remember(c.id,key,value)}
function graphEditor(c){
 if(!c)return null;
 const name=field('name',valueFor(c,'name'),{maxlength:48,'data-focus':`graph-name-${c.id}`,oninput:e=>{graphRemember(c,'name',e.target.value);refresh()}});
 const source=el('textarea',{class:'cell-source',rows:3,maxlength:2048,'aria-label':`${t('graph_source')} ${c.id}`,'data-focus':`graph-source-${c.id}`,oninput:e=>{graphRemember(c,'source',e.target.value);refresh()}});source.value=valueFor(c,'source');
 const format=valueFor(c,'source_format');
 const mode=el('select',{'aria-label':t('source_format'),onchange:e=>{const to=e.target.value;action(async()=>{const converted=await invoke('convert_source',{source:valueFor(c,'source'),fromFormat:format,toFormat:to});graphRemember(c,'source',converted);graphRemember(c,'source_format',to);await publishDrafts()})}},el('option',{value:'expression',text:t('expression_format')}),el('option',{value:'latex',text:'LaTeX'}));mode.value=format;
 const preview=el('div',{class:'latex-preview input-render'});
 bindLatexPreview(source,preview,{request:async text=>{const latex=format==='latex'?text:await invoke('convert_source',{source:text,fromFormat:'expression',toFormat:'latex'});return invoke('preview_latex',{source:latex,id:c.id})},translate:t,renderFormula:formula});
 const result=el('div',{class:'graph-output'});if(c.calculation_error)result.append(el('div',{class:'calculation-error',text:t(c.calculation_error)}));else if(c.output){result.append(el('div',{class:'small',text:t(c.output.operation)+' · '+t(c.output.trust)}),c.result_mathml?formula(c.result_mathml):el('div',{class:'numeric',text:c.result_text}));}
 const discard=()=>action(async()=>{const d=drafts.get(c.id);if(d)for(const key of ['name','source','source_format','graphBase'])delete d[key];keyboard=null;graphEditing=null;await publishDrafts()});
 const k=button('math_keyboard','keyboard',()=>{const start=source.selectionStart,end=source.selectionEnd;keyboard=keyboard?.id===c.id&&keyboard.kind==='graph'?null:{id:c.id,kind:'graph'};render();const next=root.querySelector(`[data-focus="graph-source-${c.id}"]`);if(next){next.focus();next.setSelectionRange(start,end)}},{onpointerdown:e=>e.preventDefault()});
 const out=el('section',{class:'graph-editor input-form','data-cell':c.id},el('div',{class:'graph-editor-title'},icon('edit'),el('strong',{text:t('edit_cell')}),el('span',{class:'spacer'}),button('discard_edit','close',discard)),el('div',{class:'graph-editor-row'},name,mode),source,el('div',{class:'graph-editor-row'},k,button('reference_help','info',()=>source.focus())),preview);
 if(keyboard?.id===c.id&&keyboard.kind==='graph')out.append(createMathKeyboard(source,{id:`graph-${c.id}`,mode:format,onClose:()=>{keyboard=null;render()}}));
 out.append(el('div',{class:'graph-editor-row'},button('apply','check',()=>action(async()=>{await commit(current.cells.find(cell=>cell.id===c.id));graphEditing=c.id;}),{class:'primary'}),button('discard_edit','undo',discard)),result);
 function refresh(){out.dataset.draft=String(['name','source','source_format'].some(key=>valueFor(c,key)!==c[key]));}refresh();return out;
}
function compactButton(){return button('compact','clean',()=>action(()=>mutate({method:'math.compact'})),{disabled:(current.graph_stats?.unused??0)===0?'':null})}
function showDependencies(){return action(()=>invoke('show_dependencies'));}
function dependencyWindow(){
 const zoom=el('input',{type:'range',min:.75,max:2.4,step:.05,value:dependencyZoom,'aria-label':t('graph_zoom'),'data-focus':'graph-zoom',oninput:e=>{dependencyZoom=Number(e.target.value);root.querySelector('.dependency-scroll').replaceWith(dependencyGraph())}});
 return el('section',{class:'panel dependency-window'},el('div',{class:'panel-header'},el('strong',{text:t('dependency_graph')}),el('span',{class:'spacer'}),zoom,button('save','save',()=>action(()=>fileAction('save'))),compactButton(),button('recalculate','reset',()=>action(()=>mutate({method:'math.recalculate',id:null})))),el('div',{class:'content dependency-window-content'},el('div',{class:'dependency-diagram'},dependencyGraph()),el('div',{class:'dependency-editor-pane'},graphEditor(current.cells.find(c=>c.id===current.selected)))),footer());
}

function inspector(){
 const selected=current.cells.find(c=>c.id===current.selected),out=el('div',{class:'content side-content research-inspector'});
 const links=(key,ids)=>{const row=el('div',{class:'relationship-row'},el('span',{class:'small',text:t(key)}),el('div',{class:'relationship-links'}));if(!ids?.length)row.lastElementChild.append(el('span',{class:'quiet',text:'—'}));for(const id of ids||[]){const c=current.cells.find(c=>c.id===id);if(c)row.lastElementChild.append(el('button',{type:'button',class:'reference-chip',title:c.expression,onclick:()=>action(()=>selectCell(id)),text:c.name}));}return row;};
 out.append(el('div',{class:'section-title'},el('h2',{text:t('selected_cell')}),selected?button('edit_cell','edit',()=>{graphEditing=graphEditing===selected.id?null:selected.id;render()}):null));
 if(selected){out.append(el('div',{class:'selection-heading'},el('strong',{text:selected.name}),el('span',{class:'small',text:t('kind_'+(selected.kind||'scalar'))})),links('reference_sources',selected.dependencies),links('reference_users',selected.dependents));if(selected.calculation_error)out.append(el('div',{class:'calculation-error',text:t(selected.calculation_error)}));if(graphEditing===selected.id)out.append(graphEditor(selected));}
 else out.append(el('div',{class:'small',text:t('empty')}));
 out.append(el('div',{class:'section-title'},el('h2',{text:t('dependency_graph')}),button('dependency_help','info',()=>{}),button('formula_focus','float',showDependencies)),disclosure('diagram',el('span',{text:t('show_graph')}),dependencyGraph()));
 const diagnostics=el('div',{class:'diagnostics'},el('div',{class:'section-title'},el('span',{class:'small',text:`${current.graph_stats?.live??current.node_count} / ${current.node_count} ${t('nodes')} · ${current.graph_stats?.unused??0} ${t('unused_nodes')}`}),button('seg_help','info',()=>{}),compactButton()));
 if(selected){const nodeList=el('div',{class:'node-list'});if(inspection?.cell.id===selected.id){for(const {id,node,children}of inspection.nodes.slice(0,60)){const args=Array.isArray(node.args)?node.args:[];const refs=children||args.filter(x=>typeof x==='number');nodeList.append(el('div',{class:'node'},el('span',{class:'node-id',text:`#${id}`}),el('span',{text:node.op==='reference'?'@'+node.args:node.op==='symbol'||node.op==='number'||node.op==='constant'?String(node.args):node.op==='call'?node.args[0]:node.args?.variable?`${node.op} · ${node.args.variable}`:node.op==='matrix'?`${node.args.rows}×${node.args.cols}`:node.op}),el('span',{class:'edge',text:refs.map(i=>`#${i}`).join(' ← ')})))}}else for(const symbol of selected.symbols)nodeList.append(el('div',{class:'node',text:symbol}));diagnostics.append(nodeList);}
 out.append(disclosure('seg',el('span',{text:t('seg_diagnostics')}),diagnostics));
 const list=el('ol',{class:'history-list'});for(const h of current.history)list.append(el('li',{class:'history-item'},el('span',{text:commandLabel(h.command)}),el('span',{class:'small',text:`${t(h.actor)} · ${h.branch} · r${h.revision}`})));if(!current.history.length)list.append(el('li',{class:'small',text:t('no_history')}));
 out.append(disclosure('history',el('span',{},icon('history'),el('span',{text:t('history')}),el('span',{class:'small',text:String(current.history.length)})),list));
 return out;
}

function conditionLabel(s){if(s==='real roots; symbolic solver, not a formal proof')return t('real_roots');if(s==='antiderivative; add arbitrary constant C')return t('antiderivative');return s.replace(/: (Unspecified|Real|Nonnegative|Positive|Complex|unspecified|real|nonnegative|positive|complex)(, nonzero)?$/,(_,d,n)=>': '+t(d.toLowerCase())+(n?', '+t('nonzero'):''))}
function commandLabel(c){const key=c.method?.replace('math.','');return `${t(c.operation||({define:'source',define_latex:'source',assume:'assumptions',select:'objects',checkout:'branch',rename:'name'}[key]||key))}${c.name?` · ${c.name}`:''}${c.symbol?` · ${c.symbol}`:''}`}
function panel(name){if(name==='surface')return surfacePanel();const body=name==='objects'?objects():name==='notebook'?notebook():inspector();return el('section',{class:'panel'},header(name),body,footer(),el('div',{class:'dock-preview'}))}
function surfaceSelect(id){return action(async()=>{surface=await invoke('surface_command',{command:{kind:'select',id:Number(id)}});})}
function surfacePanel(){
 document.body.classList.add('surface-body');
 const choose=el('select',{'aria-label':t('surface_cell'),onchange:e=>surfaceSelect(e.target.value)},...current.cells.map(c=>el('option',{value:c.id,text:c.name})));choose.value=String(surface?.cell||'');
 const preset=button('surface_example','add',()=>action(async()=>{let i=1;while(current.cells.some(c=>c.name==='surface'+i))i++;await mutate({method:'math.define_latex',id:null,name:'surface'+i,source:'\\sin(x)\\cos(y)'});surface=await invoke('surface_command',{command:{kind:'select',id:current.selected}});}));
 const range=(axis,index)=>el('input',{type:'number',step:'any',value:surface?.[axis+'_range']?.[index]??(index?5:-5),'aria-label':t('surface_'+axis+'_'+(index?'max':'min')),'data-focus':'range-'+axis+'-'+index});
 const xmin=range('x',0),xmax=range('x',1),ymin=range('y',0),ymax=range('y',1);
 const applyRange=()=>action(async()=>{if([xmin,xmax,ymin,ymax].some(n=>!n.value||!n.validity.valid))throw {code:'surface_range'};surface=await invoke('surface_command',{command:{kind:'range',x:[Number(xmin.value),Number(xmax.value)],y:[Number(ymin.value),Number(ymax.value)]}})});
 const row=(axis,min,max)=>el('div',{class:'surface-row'},el('span',{class:'surface-range-axis',text:axis+' ∈'}),min,el('span',{text:'…'}),max,button('apply','check',applyRange));
 const controls=el('div',{class:'surface-controls'},el('div',{class:'surface-row'},choose,preset,button('surface_wire','plot',()=>action(async()=>{surface=await invoke('surface_command',{command:{kind:'wire',enabled:!surface?.wire}})}),{'aria-pressed':surface?.wire||false}),button('surface_reset','reset',()=>action(async()=>{surface=await invoke('surface_command',{command:{kind:'reset'}})})),button('surface_help','info',()=>{})),row('x',xmin,xmax),row('y',ymin,ymax));
 const selected=current.cells.find(c=>c.id===surface?.cell);const formula=el('div',{class:'surface-formula'});if(selected){const m=el('div',{class:'surface-math'});m.innerHTML=selected.resolved_mathml||selected.mathml;if(selected.kind!=='equation')formula.append(el('span',{class:'surface-z',text:'z ='}));formula.append(m);}else formula.textContent=t('surface_empty');
 const stage=el('div',{class:'surface-stage',tabindex:0,'aria-label':t('surface_help')});
 let drag=null,pending=false,queued=null;const send=async(command)=>{queued=command;if(pending)return;pending=true;try{while(queued){const next=queued;queued=null;surface=await invoke('surface_command',{command:next});}}catch(e){queued=null;status(e.code||'error')}finally{pending=false}};
 stage.onpointerdown=e=>{if(e.button!==0||!surface)return;stage.setPointerCapture(e.pointerId);drag={x:e.clientX,y:e.clientY,orbit:[...surface.orbit]};};
 stage.onpointermove=e=>{if(drag)send({kind:'orbit',yaw:drag.orbit[0]-(e.clientX-drag.x)*.008,pitch:Math.max(-1.45,Math.min(1.45,drag.orbit[1]+(e.clientY-drag.y)*.008)),zoom:drag.orbit[2]})};
 stage.onpointerup=e=>{if(stage.hasPointerCapture(e.pointerId))stage.releasePointerCapture(e.pointerId);drag=null};stage.onpointercancel=()=>drag=null;
 stage.onwheel=e=>{e.preventDefault();if(surface)send({kind:'orbit',yaw:surface.orbit[0],pitch:surface.orbit[1],zoom:Math.max(.35,Math.min(3,surface.orbit[2]*Math.exp(-e.deltaY*.002)))})};
 stage.onkeydown=e=>{if(!surface||!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','+','-','Home'].includes(e.key))return;e.preventDefault();if(e.key==='Home'){send({kind:'reset'});return}const [yaw,pitch,zoom]=surface.orbit;send({kind:'orbit',yaw:yaw+(e.key==='ArrowLeft'?-.1:e.key==='ArrowRight'?.1:0),pitch:Math.max(-1.45,Math.min(1.45,pitch+(e.key==='ArrowUp'?.1:e.key==='ArrowDown'?-.1:0))),zoom:Math.max(.35,Math.min(3,zoom*(e.key==='+'?1.1:e.key==='-'?.9:1)))})};
 const overlay=el('div',{class:'surface-overlay'});stage.append(overlay,el('div',{class:'surface-axis-labels'}));
 const out=el('section',{class:'panel surface-panel'},header('surface'),controls,formula,stage,el('div',{class:'surface-status'}),el('div',{class:'dock-preview'}));return out;
}
function updateSurface(scope=root){
 const n=scope.querySelector('.surface-status');if(!n||!surface)return;
 n.dataset.error=String(!['ready','working'].includes(statusKey));n.title=detail;
 n.textContent=!['ready','working'].includes(statusKey)?t(statusKey):surface.status==='ready'?`${surface.samples} · float64 · r${surface.revision}`:t(surface.status);
 const overlay=scope.querySelector('.surface-overlay');if(overlay){overlay.replaceChildren();if(surface.status!=='ready')overlay.append(el('span',{class:'surface-message',text:t(surface.status)}));if(surface.heights)overlay.append(el('span',{class:'surface-heights',text:`z ∈ [${surface.heights.map(v=>Number(v.toPrecision(5))).join(', ')}]`}),el('span',{class:'surface-legend',text:`x · y · z  /  ${t('approximate')}${surface.omitted?' · '+surface.omitted+' '+t('surface_omitted'):''}`}));}
 renderSurfaceAxes(scope.querySelector('.surface-stage'),surfaceAxes,surface);
}
function render(){if(!current&&!view.startsWith('split'))return;
 const focus=document.activeElement?.dataset.focus,selection=document.activeElement?.selectionStart,selectionEnd=document.activeElement?.selectionEnd;const scrolls=[...root.querySelectorAll('.content')].map(e=>e.scrollTop);
 if(view.startsWith('split')){const focused=document.activeElement?.classList.contains('splitter');root.replaceChildren(bindSplitter());if(focused)root.firstElementChild.focus();return}
 if(view==='toolbar')root.replaceChildren(toolbar());else if(view==='dependencies')root.replaceChildren(dependencyWindow());else if(view==='preview')root.replaceChildren(el('div',{class:'preview-shell'},toolbar(),el('div',{class:'preview-panels'},...['objects','notebook','inspector'].map(panel))));else root.replaceChildren(panel(view));
 if(view==='surface')updateSurface();
 const dependencyDialog=document.querySelector('dialog.dependency-dialog');if(dependencyDialog)dependencyDialog.querySelector('.dependency-scroll').replaceWith(dependencyGraph());
 root.querySelectorAll('.content').forEach((e,i)=>e.scrollTop=scrolls[i]||0);
 if(revealCell!=null&&(view==='notebook'||view==='preview')){const cell=root.querySelector(`.cell[data-cell="${revealCell}"]`);if(cell){cell.scrollIntoView({block:'nearest',behavior:'auto'});revealCell=null;}}
 if(focus){const f=[...root.querySelectorAll('[data-focus]')].find(n=>n.dataset.focus===focus);f?.focus();if(f&&selection!=null)f.setSelectionRange(selection,selectionEnd??selection)}
 if(!api||busy)root.querySelectorAll('button,input,select,textarea').forEach(e=>e.disabled=true);
}
async function showJson(){if(!api)return;await action(async()=>{const text=await invoke('export_document');const textarea=el('textarea',{'aria-label':t('json'),readonly:''});textarea.value=text;const dialog=el('dialog',{},el('div',{class:'dialog-head'},el('strong',{text:t('json')}),button('close','close',()=>dialog.close())),textarea,button('copy','json',()=>textarea.select()));dialog.addEventListener('close',()=>dialog.remove());document.body.append(dialog);dialog.showModal();});}
root.addEventListener('click',e=>{const reference=e.target.closest?.('[data-reference]');if(reference){const cell=current.cells.find(c=>c.name===reference.dataset.reference);if(cell)action(()=>selectCell(cell.id));}});
root.addEventListener('keydown',e=>{if(e.key==='Enter'&&!e.ctrlKey&&!e.metaKey&&e.target.matches?.('[data-reference]')){e.preventDefault();e.target.dispatchEvent(new MouseEvent('click',{bubbles:true}));}});
document.addEventListener('keydown',e=>{if((e.metaKey||e.ctrlKey)&&!e.shiftKey&&e.key.toLowerCase()==='n'){e.preventDefault();action(()=>fileAction('new_document'))} if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='o'){e.preventDefault();action(()=>fileAction('open_document'))} if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='s'){e.preventDefault();action(()=>fileAction(e.shiftKey?'save_as':'save'))}if((e.metaKey||e.ctrlKey)&&e.key==='z'&&!e.target.closest('input,textarea')){e.preventDefault();action(()=>mutate({method:e.shiftKey?'math.redo':'math.undo'}))}if((e.metaKey||e.ctrlKey)&&e.key==='Enter'){const c=current.cells.find(c=>c.id===Number(e.target.closest('[data-cell]')?.dataset.cell));if(c){e.preventDefault();action(async()=>{await commit(c);await mutate({method:'math.transform',id:c.id,operation:'compute',variable:'x',bindings:{}})})}}});
async function start(){if(!api){try{current=await(await fetch('./preview.json')).json();setLocale('en');layout={revision:0,locale:'en',floating:[],floating_groups:[],root:{kind:'split',first:{kind:'tabs',panels:['objects'],active:'objects'},second:{kind:'split',first:{kind:'tabs',panels:['notebook'],active:'notebook'},second:{kind:'tabs',panels:['inspector'],active:'inspector'}}}};render()}catch{root.textContent=t('preview')}return}
 try{await api.event.listen('symbio://file-busy',e=>{root.inert=e.payload});if(view==='surface'){document.body.classList.add('surface-body');surface=await invoke('surface_state');await api.event.listen('symbio://surface-axes',e=>{surfaceAxes=e.payload;renderSurfaceAxes(root.querySelector('.surface-stage'),surfaceAxes,surface)});await api.event.listen('symbio://surface',e=>{const previous=surface?.cell;surface=e.payload;if(previous!==surface.cell&&!busy)render();else updateSurface()})}await api.event.listen('symbio://prepare-save',()=>{if(pendingDrafts()&&!busy)action(flushDrafts)});await api.event.listen('symbio://changed',e=>accept(e.payload));await api.event.listen('symbio://layout',e=>applyLayout(e.payload));await api.event.listen('symbio://close-blocked',()=>status('unsaved_changes'));await api.event.listen('symbio://panel-error',e=>status(e.payload));await api.event.listen('symbio://dock-preview',e=>document.body.dataset.drop=e.payload?.target===view?e.payload.zone:'');bindDock();if(!view.startsWith('split'))current=await invoke('snapshot');applyLayout(await invoke('workspace_state'));render();refreshInspection();}catch(e){root.textContent=t(e.code||'error')};}
start();
