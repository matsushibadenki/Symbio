// Layout consumes a Rust-owned DAG; it never evaluates or owns mathematical state.
export function dependencyLayout(cells, edges) {
 const ids=new Set(cells.map(c=>c.id)),inputs=new Map(cells.map(c=>[c.id,[]]));
 for(const edge of edges){if(!ids.has(edge.from)||!ids.has(edge.to))throw new Error('invalid_dependency');inputs.get(edge.to).push(edge.from)}
 const levels=new Map(),visiting=new Set();
 function rank(id){if(levels.has(id))return levels.get(id);if(visiting.has(id))throw new Error('dependency_cycle');visiting.add(id);const value=Math.max(-1,...inputs.get(id).map(rank))+1;visiting.delete(id);levels.set(id,value);return value}
 for(const cell of cells)rank(cell.id);
 const rows=[];for(const cell of cells){const level=levels.get(cell.id);(rows[level]??=[]).push(cell)}
 const width=Math.max(260,...rows.map(row=>row.length*126+80)),height=Math.max(90,rows.length*86+16);
 const nodes=rows.flatMap((row,level)=>row.map((cell,index)=>({id:cell.id,x:(width-row.length*126)/2+index*126+6,y:level*86+12,cell})));
 const positions=new Map(nodes.map(n=>[n.id,n]));
 return {width,height,nodes,edges:edges.map(edge=>{const a=positions.get(edge.from),b=positions.get(edge.to);if(b.y-a.y>86){const lane=Math.min(a.x,b.x)-38;return {...edge,path:`M ${a.x} ${a.y+25} C ${lane} ${a.y+25}, ${lane} ${b.y+25}, ${b.x} ${b.y+25}`,arrow:`M ${b.x-5} ${b.y+21} l 5 4 l -5 4`}}
 return {...edge,path:`M ${a.x+57} ${a.y+50} C ${a.x+57} ${a.y+70}, ${b.x+57} ${b.y-20}, ${b.x+57} ${b.y}`,arrow:`M ${b.x+53} ${b.y-5} l 4 5 l 4 -5` }})};
}
