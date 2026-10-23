import {test} from 'node:test';import assert from 'node:assert/strict';
import {dependencyLayout} from '../apps/desktop/ui/dependency-layout.js';
test('diamond dependencies point downward and independent cells do not overlap',()=>{
 const cells=[1,2,3,4,5].map(id=>({id}));const edges=[{from:1,to:2},{from:1,to:3},{from:2,to:4},{from:3,to:4}];const graph=dependencyLayout(cells,edges);
 const nodes=new Map(graph.nodes.map(n=>[n.id,n]));for(const e of edges)assert.ok(nodes.get(e.from).y<nodes.get(e.to).y);
 for(const a of graph.nodes)for(const b of graph.nodes)if(a.id!==b.id&&a.y===b.y)assert.ok(Math.abs(a.x-b.x)>=120);
 assert.equal(graph.edges.length,4);assert.ok(graph.width>=260);
});
test('invalid edges and cycles are rejected',()=>{
 assert.throws(()=>dependencyLayout([{id:1}],[{from:1,to:2}]));
 assert.throws(()=>dependencyLayout([{id:1},{id:2}],[{from:1,to:2},{from:2,to:1}]));
});
