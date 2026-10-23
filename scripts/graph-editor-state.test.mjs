import test from 'node:test';
import assert from 'node:assert/strict';
import {definitionOf,definitionChanged} from '../apps/desktop/ui/graph-editor-state.js';
test('a graph draft detects concurrent source, format and name edits',()=>{
 const c={name:'f',source:'x+1',source_format:'expression'},base=definitionOf(c);
 for(const edit of [{source:'x+2'},{name:'g'},{source_format:'latex'}])assert.equal(definitionChanged(base,{...c,...edit}),true);
});
test('selection, recalculation and legacy format defaults do not invalidate a draft',()=>{
 const c={name:'f',source:'x+1'},base=definitionOf(c);
 assert.equal(definitionChanged(base,{...c,source_format:'expression',revision:5,output:{value:4}}),false);
 assert.equal(definitionChanged(undefined,c),false);
});
