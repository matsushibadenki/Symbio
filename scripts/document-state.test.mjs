import test from 'node:test';
import assert from 'node:assert/strict';
import {compareDocumentSnapshot as compare} from '../apps/desktop/ui/document-state.js';
test('document switches accept lower revisions and reject old document responses',()=>{
 assert.equal(compare({generation:1,revision:99},{generation:2,revision:0}),'replace');
 assert.equal(compare({generation:2,revision:0},{generation:1,revision:100}),'ignore');
 assert.equal(compare(null,{generation:1,revision:0}),'replace');
});
test('within one document revision ordering is retained',()=>{
 assert.equal(compare({generation:2,revision:10},{generation:2,revision:9}),'ignore');
 assert.equal(compare({generation:2,revision:10},{generation:2,revision:10}),'update');
 assert.equal(compare({generation:2,revision:10},{generation:2,revision:11}),'update');
});
