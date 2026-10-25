//! Read-only structural comparison. Never claims mathematical equivalence.
use crate::{Assumption, Cell, Error, Output, Project, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct BranchComparison {
    pub revision: u64,
    pub base: String,
    pub target: String,
    pub cells: Vec<CellDifference>,
    pub assumptions: Vec<AssumptionDifference>,
    pub unchanged_cells: usize,
}
#[derive(Debug, Serialize)]
pub struct CellDifference {
    pub id: u64,
    pub status: &'static str,
    pub fields: Vec<&'static str>,
    pub base: Option<ComparedCell>,
    pub target: Option<ComparedCell>,
}
#[derive(Debug, Serialize)]
pub struct ComparedCell {
    pub name: String,
    pub source: String,
    pub source_format: crate::SourceFormat,
    pub mathml: String,
    pub resolved_mathml: String,
    pub result_mathml: Option<String>,
    pub calculation: Option<crate::Calculation>,
    pub calculation_error: Option<String>,
    pub note: String,
    pub output: Option<Output>,
}
#[derive(Debug, Serialize)]
pub struct AssumptionDifference {
    pub symbol: String,
    pub base: Option<Assumption>,
    pub target: Option<Assumption>,
}
fn same_output(a: &Option<Output>, b: &Option<Output>) -> bool {
    // Node IDs are in the same document graph. Revision timestamps are not evidence differences.
    serde_json::to_value(a).expect("output") == serde_json::to_value(b).expect("output")
}
fn view(p: &Project, c: &Cell) -> ComparedCell {
    ComparedCell {
        name: c.name.clone(),
        source: c.source.clone(),
        source_format: c.source_format.clone(),
        mathml: p.graph.definition_mathml(c.root, &c.name),
        resolved_mathml: p.graph.mathml(c.resolved_root.unwrap_or(c.root)),
        result_mathml: c.output.as_ref().and_then(|o| {
            o.root.map(|r| {
                if o.trust == crate::Trust::Approximate {
                    p.graph.approximate_mathml(r)
                } else {
                    p.graph.mathml(r)
                }
            })
        }),
        calculation: c.calculation.clone(),
        calculation_error: c.calculation_error.clone(),
        note: c.note.clone(),
        output: c.output.clone(),
    }
}
pub fn compare(p: &Project, base: &str, target: &str) -> Result<BranchComparison> {
    let a = p
        .branches
        .get(base)
        .ok_or_else(|| Error::Invalid("comparison branch".into()))?;
    let b = p
        .branches
        .get(target)
        .ok_or_else(|| Error::Invalid("comparison branch".into()))?;
    let left: BTreeMap<_, _> = a.cells.iter().map(|c| (c.id, c)).collect();
    let right: BTreeMap<_, _> = b.cells.iter().map(|c| (c.id, c)).collect();
    let ids: BTreeSet<_> = left.keys().chain(right.keys()).copied().collect();
    let mut cells = Vec::new();
    let mut unchanged_cells = 0;
    for id in ids {
        let x = left.get(&id).copied();
        let y = right.get(&id).copied();
        let mut fields = Vec::new();
        let status = match (x, y) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            (Some(x), Some(y)) => {
                if x.name != y.name {
                    fields.push("name");
                }
                if x.root != y.root {
                    fields.push("definition");
                }
                if x.source != y.source || x.source_format != y.source_format {
                    fields.push("source");
                }
                if x.resolved_root.unwrap_or(x.root) != y.resolved_root.unwrap_or(y.root) {
                    fields.push("resolved");
                }
                if x.calculation != y.calculation {
                    fields.push("calculation");
                }
                if !same_output(&x.output, &y.output) {
                    fields.push("result");
                }
                if x.calculation_error != y.calculation_error {
                    fields.push("calculation_error");
                }
                if x.note != y.note {
                    fields.push("note");
                }
                if fields.is_empty() {
                    unchanged_cells += 1;
                    continue;
                }
                "changed"
            }
            (None, None) => unreachable!(),
        };
        cells.push(CellDifference {
            id,
            status,
            fields,
            base: x.map(|c| view(p, c)),
            target: y.map(|c| view(p, c)),
        });
    }
    let symbols: BTreeSet<_> = a
        .assumptions
        .keys()
        .chain(b.assumptions.keys())
        .cloned()
        .collect();
    let assumptions = symbols
        .into_iter()
        .filter_map(|symbol| {
            let x = a.assumptions.get(&symbol);
            let y = b.assumptions.get(&symbol);
            (x != y).then(|| AssumptionDifference {
                symbol,
                base: x.cloned(),
                target: y.cloned(),
            })
        })
        .collect();
    Ok(BranchComparison {
        revision: p.revision,
        base: base.into(),
        target: target.into(),
        cells,
        assumptions,
        unchanged_cells,
    })
}
