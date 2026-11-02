import test from 'node:test';
import assert from 'node:assert/strict';
import {surfaceFrameMatches} from '../apps/desktop/ui/surface-axes.js';
const view=()=>({generation:1,cell:2,comparisons:[3],revision:7,x_range:[-1,1],y_range:[-2,2],orbit:[.7,.6,1],heights:[-3,3],wire:true,status:'ready'});
test('axis labels require exact current document, comparison, revision and camera stamps',()=>{
 const v=view();assert.equal(surfaceFrameMatches({...v},v),true);
 for(const patch of [{generation:2},{cell:3},{comparisons:[]},{comparisons:[4]},{revision:8},{x_range:[-5,5]},{y_range:[0,2]},{orbit:[1,.6,1]},{heights:[0,3]}])assert.equal(surfaceFrameMatches({...v,...patch},v),false);
});
test('working, unavailable and hidden axes never display old projected labels',()=>{
 const v=view();for(const patch of [{status:'working'},{status:'gpu_unavailable'},{wire:false}])assert.equal(surfaceFrameMatches(v,{...v,...patch}),false);
 assert.equal(surfaceFrameMatches(null,v),false);assert.equal(surfaceFrameMatches({...v,generation:undefined},{...v,generation:undefined}),false);
});
