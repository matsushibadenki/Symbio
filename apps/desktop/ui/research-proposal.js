export function researchFormula(tool, args, result) {
  if (result.ok !== true || typeof args?.source !== 'string') return null;
  if (tool === 'math_preview_latex' && result.preview?.mathml) {
    return { source: args.source, format: 'latex', mathml: result.preview.mathml, assumptions: {} };
  }
  const cell = result.cell;
  if (tool !== 'math_calculate' || !cell?.definition_mathml || cell.calculation_error) return null;
  if (!['expression', 'latex'].includes(cell.source_format)) return null;
  return {
    source: cell.source,
    format: cell.source_format,
    mathml: cell.definition_mathml,
    assumptions: args.assumptions ?? {},
  };
}

export function researchBranchName(branches) {
  let suffix = 1;
  while (branches.includes(`research-${suffix}`)) suffix++;
  return `research-${suffix}`;
}

export function researchEdit(proposal, baseline, current, branch) {
  if (baseline.generation !== current.generation ||
      baseline.revision !== current.revision ||
      baseline.active !== current.active) return null;

  const names = new Set(current.cells.map(cell => cell.name));
  let suffix = 1;
  while (names.has(`research${suffix}`)) suffix++;
  const name = `research${suffix}`;
  const command = branch ? {
    method: 'math.branch_research',
    branch,
    name,
    source: proposal.source,
    source_format: proposal.format,
    assumptions: proposal.assumptions,
  } : {
    method: proposal.format === 'latex' ? 'math.define_latex' : 'math.define',
    id: null,
    name,
    source: proposal.source,
  };
  return {
    generation: baseline.generation,
    request: {
      expected_revision: baseline.revision,
      actor: 'ai',
      command,
    },
  };
}
