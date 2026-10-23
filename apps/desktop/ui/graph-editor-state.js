// A local draft must not overwrite a definition changed in another native panel.
export const definitionOf=cell=>({name:cell.name,source:cell.source,source_format:cell.source_format||'expression'});
export const definitionChanged=(base,cell)=>Boolean(base&&Object.entries(definitionOf(cell)).some(([key,value])=>base[key]!==value));
