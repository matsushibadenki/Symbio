// Preview has no document ownership: Rust builds an isolated SEG and returns safe MathML.
// Results from old edits / detached panels must never replace the current draft preview.
export function bindLatexPreview(target, host, {request, translate, renderFormula}) {
 let timer, generation=0;
 function schedule() {
  const version=++generation, source=target.value;
  clearTimeout(timer);
  host.replaceChildren();
  host.removeAttribute('title');
  host.dataset.state='pending';
  host.textContent=translate(source.trim()?'working':'latex_empty');
  if(!source.trim())return;
  timer=setTimeout(async()=>{
   if(!host.isConnected||version!==generation)return;
   try {
    const result=await request(source);
    if(!host.isConnected||version!==generation)return;
    host.dataset.state='ready';
    host.replaceChildren(renderFormula(result.mathml));
    host.title=result.expression;
   } catch(error) {
    if(!host.isConnected||version!==generation)return;
    host.dataset.state='error';
    host.textContent=translate(error.code||'parse_error')+(error.detail?` ${error.detail}`:'');
    host.removeAttribute('title');
   }
  },180);
 }
 target.addEventListener('input',schedule);
 schedule();
}
