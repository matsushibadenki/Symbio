// Pure editing rules. The Rust kernel remains the only evaluator.
export function editSelection(value, start, end, key, mode='expression') {
  start=Math.max(0,Math.min(value.length,start));end=Math.max(start,Math.min(value.length,end));
  const selected=value.slice(start,end);
  let insert=key.code??'',from=insert.length,to=from;
  if(mode==='latex'){
    insert=insert.replace(/\{\}/g,'{□}').replace(/\[\]/g,'[□]').replace(/ {2}/g,' □ ');
    if(selected&&insert.includes('□')&&!insert.startsWith('^')&&!insert.startsWith('_'))insert=insert.replace('□',()=>selected);
    else if(selected&&insert.startsWith('^'))insert=(/^[A-Za-z0-9]$/.test(selected)?selected:`\\left(${selected}\\right)`)+insert;
    else if(selected&&insert.startsWith('_'))insert=selected+insert;
    from=insert.indexOf('□');if(from<0)from=insert.length;to=Math.min(insert.length,from+1);
  }else if(key.template==='fraction'){
    const numerator=selected||'x';insert=`(${numerator})/(1)`;from=selected?insert.length-2:1;to=selected?from+1:2;
  }else if(key.template==='power'){
    insert=`(${selected||'x'})^(2)`;from=insert.length-2;to=from+1;
  }else if(key.wrap){
    insert=key.wrap.replace('$',()=>selected||'x');from=selected?insert.length:key.wrap.indexOf('$');to=selected?from:from+1;
  }
  return {value:value.slice(0,start)+insert+value.slice(end),start:start+from,end:start+to};
}
export function eraseSelection(value,start,end,backward){
  if(start===end){
    if(backward)start-=Array.from(value.slice(0,start)).at(-1)?.length||0;
    else end+=Array.from(value.slice(end))[0]?.length||0;
  }
  return {value:value.slice(0,start)+value.slice(end),start,end:start};
}
export class DraftHistory {
  constructor(value){this.entries=[value];this.index=0;}
  record(value){if(this.entries[this.index]===value)return;this.entries=this.entries.slice(0,this.index+1);this.entries.push(value);if(this.entries.length>32)this.entries.shift();this.index=this.entries.length-1;}
  move(delta){this.index=Math.max(0,Math.min(this.entries.length-1,this.index+delta));return this.entries[this.index];}
}
