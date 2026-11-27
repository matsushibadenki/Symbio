import assert from 'node:assert/strict';
import test from 'node:test';
import { researchEdit, researchFormula } from '../apps/desktop/ui/research-proposal.js';

test('approval inserts the authored formula without importing scratch state', () => {
  const source = String.raw`\sqrt{x^2}`;
  const assumptions = { x: { domain: 'real' } };
  const candidate = researchFormula('math_calculate', { source, assumptions }, {
    ok: true,
    cell: {
      source,
      source_format: 'latex',
      definition_mathml: '<math><msqrt><mi>x</mi></msqrt></math>',
      result_text: 'abs(x)',
    },
  });
  assert.deepEqual(candidate.assumptions, assumptions);
  const current = { generation: 2, revision: 7, active: 'main', cells: [{ name: 'research1' }] };
  assert.deepEqual(researchEdit(candidate, current, current), {
    generation: 2,
    request: {
      expected_revision: 7,
      actor: 'ai',
      command: { method: 'math.define_latex', id: null, name: 'research2', source },
    },
  });
  for (const change of [{ generation: 3 }, { revision: 8 }, { active: 'branch' }]) {
    assert.equal(researchEdit(candidate, current, { ...current, ...change }), null);
  }
});

test('failed calculations and unsupported tools do not produce a proposal', () => {
  const args = { source: '1/0' };
  assert.equal(researchFormula('math_calculate', args, { ok: false }), null);
  assert.equal(researchFormula('math_calculate', args, {
    ok: true, cell: { definition_mathml: '<math/>', calculation_error: 'domain_error' },
  }), null);
  assert.equal(researchFormula('unknown', args, { ok: true, preview: { mathml: '<math/>' } }), null);
});

test('branch approval submits one command carrying the reviewed assumptions', () => {
  const current = { generation: 1, revision: 2, active: 'main', cells: [] };
  const assumptions = { x: { domain: 'real' } };
  assert.deepEqual(researchEdit({
    source: 'sqrt(x^2)', format: 'expression', assumptions,
  }, current, current, 'research-2').request.command, {
    method: 'math.branch_research',
    branch: 'research-2',
    name: 'research1',
    source: 'sqrt(x^2)',
    source_format: 'expression',
    assumptions,
  });
});
