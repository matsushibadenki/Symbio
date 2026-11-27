import assert from 'node:assert/strict';
import test from 'node:test';
import { researchContext, researchPrompt } from '../apps/desktop/ui/research-context.js';

const snapshot = {
  name: 'Private notebook',
  cells: [
    { id: 1, name: 'base', source: '42', source_format: 'expression' },
    {
      id: 2, name: 'f', source: '@base+x', source_format: 'expression',
      note: 'Private note', resolved_expression: '42+x', result_text: '42+x',
    },
  ],
  assumptions: { x: { domain: 'real' }, y: { domain: 'positive' } },
  history: ['Private history'],
};

test('empty or unknown selections send the question unchanged', () => {
  const context = researchContext(snapshot, new Set([999]), new Set(['unknown']));
  assert.equal(context, '');
  assert.equal(researchPrompt('Find the derivative', context), 'Find the derivative');
});

test('only explicitly selected authored source and assumptions are serialized', () => {
  const context = researchContext(snapshot, new Set([2]), new Set(['x']));
  assert.deepEqual(JSON.parse(context), {
    cells: [{ name: 'f', source: '@base+x', source_format: 'expression' }],
    assumptions: { x: { domain: 'real' } },
  });
  assert.ok(researchPrompt('Find the derivative', context).endsWith(context));
});
