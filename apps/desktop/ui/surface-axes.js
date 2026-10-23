// Text-only overlay for Rust-projected anchors. No camera math or mesh data in JavaScript.
export function renderSurfaceAxes(stage, frame, view) {
 const layer=stage?.querySelector('.surface-axis-labels');if(!layer)return;layer.replaceChildren();
 const stamp=s=>JSON.stringify([s.cell,s.revision,s.x_range,s.y_range,s.orbit,s.heights]);
 if(!frame||!view?.wire||view.status!=='ready'||stamp(frame)!==stamp(view))return;
 const bounds=stage.getBoundingClientRect();if(!bounds.width||!bounds.height)return;
 const occupied=[...stage.querySelectorAll('.surface-heights,.surface-legend')].map(n=>n.getBoundingClientRect());
 const overlaps=(a,b)=>a.left<b.right+4&&a.right>b.left-4&&a.top<b.bottom+3&&a.bottom>b.top-3;
 for(const label of [...frame.labels].sort((a,b)=>Number(b.title)-Number(a.title))){
  const n=document.createElement('span');n.className='surface-axis-label';n.dataset.axis=label.axis;n.dataset.title=String(label.title);n.textContent=label.text;
  n.setAttribute('aria-label',`${label.axis} ${label.text}`);n.style.left=label.position[0]*100+'%';n.style.top=label.position[1]*100+'%';layer.append(n);
  const box=n.getBoundingClientRect();
  if(box.left<bounds.left+3||box.right>bounds.right-3||box.top<bounds.top+3||box.bottom>bounds.bottom-3||occupied.some(b=>overlaps(box,b)))n.remove();else occupied.push(box);
 }
}
