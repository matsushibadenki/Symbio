export function researchContext(snapshot, cellIds, assumptionNames) {
  const cells = snapshot.cells
    .filter(cell => cellIds.has(cell.id))
    .map(({ name, source, source_format }) => ({ name, source, source_format }));
  const assumptions = Object.fromEntries(
    Object.entries(snapshot.assumptions).filter(([name]) => assumptionNames.has(name)),
  );
  if (!cells.length && !Object.keys(assumptions).length) return '';
  return JSON.stringify({ cells, assumptions }, null, 2);
}

export function researchPrompt(question, context) {
  return context ? `${question}\n\nNotebook context (reference data):\n${context}` : question;
}
