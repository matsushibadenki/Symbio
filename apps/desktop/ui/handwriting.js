let inkRevision=Date.now();
// Reusable editor shell: host supplies recognition, validation and commit adapters.
// Only input gestures and editable LaTeX live here; tensors/model execution stay native.
export async function openHandwriting({translate:t, configure, readSettings, pickPath, updateInk, recognize, preview, commit, pending, generationValid}) {
 await import('./vendor/mathlive/mathlive.min.mjs');
 const Mathfield=customElements.get('math-field');
 Mathfield.fontsDirectory=new URL('./vendor/mathlive/fonts',import.meta.url).href;
 Mathfield.soundsDirectory=null;
 const make=(tag,attrs={},text='')=>{const e=document.createElement(tag);for(const [k,v] of Object.entries(attrs))e.setAttribute(k,v);e.textContent=text;return e};
 const dialog=make('dialog',{class:'ink-dialog','aria-label':t('handwriting')});
 const head=make('div',{class:'ink-tools'});head.append(make('strong',{},t('handwriting')));
 const status=make('div',{class:'small',role:'status'});
 const info=make('button',{type:'button',title:t('handwriting_help'),'aria-label':t('handwriting_help')},'ⓘ');
 const close=make('button',{type:'button','aria-label':t('close')},'×');head.append(info,close);
 const settings=make('details');settings.append(make('summary',{},t('handwriting_setup')));
 const models=make('input',{'aria-label':t('handwriting_models'),placeholder:t('handwriting_models')});
 const runtime=make('input',{'aria-label':t('handwriting_runtime'),placeholder:t('handwriting_runtime')});
 const browseModels=make('button',{type:'button',title:t('handwriting_models'),'aria-label':t('handwriting_models')},'📂');
 const browseRuntime=make('button',{type:'button',title:t('handwriting_runtime'),'aria-label':t('handwriting_runtime')},'📂');
 const modelRow=make('div',{class:'ink-path'});modelRow.append(models,browseModels);
 const runtimeRow=make('div',{class:'ink-path'});runtimeRow.append(runtime,browseRuntime);
 const load=make('button',{type:'button'},t('apply'));settings.append(modelRow,runtimeRow,load);
 const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 1000 320');svg.setAttribute('class','ink-pad');svg.setAttribute('aria-label',t('handwriting_pad'));
 const tools=make('div',{class:'ink-tools'});
 const undo=make('button',{type:'button','aria-label':t('undo'),title:t('undo')},'↶');
 const clear=make('button',{type:'button','aria-label':t('handwriting_clear'),title:t('handwriting_clear')},'⌫');
 const infer=make('button',{type:'button',class:'primary'},t('handwriting_recognize'));
 tools.append(undo,clear,infer);
 const choices=make('div',{class:'ink-candidates',role:'group','aria-label':t('handwriting_candidates')});
 const editor=make('math-field',{'virtual-keyboard-mode':'manual','aria-label':t('handwriting_candidate')});
 const source=make('textarea',{'aria-label':t('latex'),rows:'2',spellcheck:'false'});
 const math=make('div',{class:'ink-preview'});
 const add=make('button',{type:'button',class:'primary',disabled:''},t('handwriting_insert'));
 dialog.append(head,settings,svg,tools,choices,editor,source,math,status,add);document.body.append(dialog);
 let strokes=[],active=null,revision=++inkRevision,epoch=0,valid=false,queue=Promise.resolve(),working=false,closed=false;
 const report=e=>{status.textContent=e?.detail||e?.message||String(e)};
 const invalidate=()=>{choices.replaceChildren();++epoch;valid=false;add.disabled=true;if(source.value)status.textContent=t('handwriting_stale')};
 const dirty=()=>pending(strokes.length>0||source.value.length>0);
 const redraw=()=>{svg.replaceChildren();for(const stroke of strokes){const path=document.createElementNS(svg.namespaceURI,'path');path.setAttribute('d',stroke.map((p,i)=>(i?'L':'M')+p.x+','+p.y).join(' '));svg.append(path)}};
 const sync=()=>{const ink={strokes:structuredClone(strokes)};const r=revision=++inkRevision;queue=queue.catch(()=>{}).then(()=>updateInk(ink,r));return queue};
 const validate=async value=>{const token=++epoch;valid=false;add.disabled=true;math.replaceChildren();source.value=value;dirty();if(!value.trim())return;try{const result=await preview(value);if(closed||token!==epoch)return;math.innerHTML=result.mathml;valid=true;add.disabled=working||active!==null;status.textContent='';}catch(e){if(token===epoch)report(e)}};
 editor.addEventListener('input',()=>validate(editor.value));source.addEventListener('input',()=>{editor.value=source.value;validate(source.value)});
 const point=e=>{const r=svg.getBoundingClientRect();return {x:Math.max(0,Math.min(1000,(e.clientX-r.left)*1000/r.width)),y:Math.max(0,Math.min(320,(e.clientY-r.top)*320/r.height))}};
 svg.addEventListener('pointerdown',e=>{if(working||active||!e.isPrimary||e.button!==0||strokes.length>=256)return;e.preventDefault();svg.setPointerCapture(e.pointerId);active=[point(e)];strokes.push(active);redraw();dirty();invalidate();});
 svg.addEventListener('pointermove',e=>{if(!active||!e.isPrimary)return;if(strokes.reduce((n,s)=>n+s.length,0)>=16000)return;active.push(point(e));redraw()});
 const end=()=>{if(!active)return;if(active.length===1)active.push({...active[0],x:active[0].x+0.1});active=null;sync().catch(report)};
 svg.addEventListener('pointerup',end);svg.addEventListener('pointercancel',end);
 undo.onclick=()=>{if(working)return;strokes.pop();invalidate();redraw();sync().catch(report);dirty()};
 clear.onclick=()=>{if(working)return;strokes=[];active=null;choices.replaceChildren();++epoch;redraw();editor.value='';source.value='';math.replaceChildren();valid=false;add.disabled=true;sync().catch(report);dirty()};
 const run=async fn=>{if(working)return;working=true;add.disabled=true;load.disabled=infer.disabled=undo.disabled=clear.disabled=browseModels.disabled=browseRuntime.disabled=true;status.textContent=t('working');try{await fn()}catch(e){report(e)}finally{working=false;load.disabled=infer.disabled=undo.disabled=clear.disabled=browseModels.disabled=browseRuntime.disabled=false;add.disabled=!valid||active!==null}};
 const browse=(kind,input)=>run(async()=>{const path=await pickPath(kind);if(!closed&&path&&generationValid())input.value=path;status.textContent=''});
 browseModels.onclick=()=>browse('models',models);browseRuntime.onclick=()=>browse('runtime',runtime);
 load.onclick=()=>run(async()=>{await configure(models.value,runtime.value);settings.open=false;status.textContent=t('ready')});
 infer.onclick=()=>run(async()=>{await queue;if(!strokes.length)return;const token=epoch;const r=revision;const result=await recognize(r);if(closed||token!==epoch||!generationValid())return;choices.replaceChildren();const candidates=(result.candidates?.length?result.candidates:[result]).slice(0,3);
 const choose=async candidate=>{editor.value=candidate.latex;await validate(candidate.latex);if(!closed)status.textContent=(candidate.complete?'':t('handwriting_incomplete')+' · ')+result.elapsed_ms+' ms'};
 for(const [i,candidate] of candidates.entries()){
  const button=make('button',{type:'button',title:t('handwriting_candidates_help'),'aria-label':t('handwriting_candidate')+' '+(i+1)},String(i+1));
  const formula=make('math-field',{inert:'','read-only':'','virtual-keyboard-mode':'off','aria-hidden':'true',tabindex:'-1'});formula.value=candidate.latex;button.append(formula);
  button.onclick=()=>{if(working||closed||revision!==r||!generationValid())return;for(const b of choices.children)b.setAttribute('aria-pressed',String(b===button));return choose(candidate)};
  button.setAttribute('aria-pressed',String(i===0));choices.append(button);
 }
 await choose(candidates[0]);});
 add.onclick=()=>run(async()=>{if(!valid||active!==null||!generationValid())return;await commit(editor.value);strokes=[];source.value='';pending(false);dialog.close()});
 close.onclick=()=>dialog.close();dialog.addEventListener('cancel',e=>{if(working){e.preventDefault();return;}});
 dialog.addEventListener('close',()=>{closed=true;++epoch;pending(false);updateInk({strokes:[]},revision=++inkRevision).catch(()=>{});dialog.remove()});
 dialog.showModal();
 if(readSettings)try{const saved=await readSettings();if(!closed){models.value=saved.models||'';runtime.value=saved.runtime||'';}}catch(e){report(e)}
}
