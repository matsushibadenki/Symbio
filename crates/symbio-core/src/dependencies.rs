//! Branch-local definition references and deterministic, bounded recalculation.
use crate::{
    graph::{Graph, Id, Node},
    *,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize)]
pub struct DependencyEdge {
    pub from: u64,
    pub to: u64,
}
pub struct DependencyGraph {
    pub inputs: BTreeMap<u64, Vec<u64>>,
    pub order: Vec<u64>,
}
pub fn references(g: &Graph, root: Id) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut todo = vec![root];
    while let Some(id) = todo.pop() {
        if seen.insert(id) {
            let Some(node) = g.nodes.get(id) else {
                continue;
            };
            if let Node::Reference(name) = node {
                names.insert(name.clone());
            }
            todo.extend(node.children());
        }
    }
    names
}
impl DependencyGraph {
    pub fn build(g: &Graph, branch: &Branch) -> Result<Self> {
        let names: BTreeMap<_, _> = branch
            .cells
            .iter()
            .map(|c| (c.name.as_str(), c.id))
            .collect();
        if names.len() != branch.cells.len() {
            return Err(Error::Invalid("duplicate cell name".into()));
        }
        let mut inputs = BTreeMap::new();
        for cell in &branch.cells {
            if cell.root >= g.nodes.len() {
                return Err(Error::Invalid("cell root".into()));
            }
            let ids = references(g, cell.root)
                .into_iter()
                .map(|name| {
                    names
                        .get(name.as_str())
                        .copied()
                        .ok_or(Error::MissingReference(name))
                })
                .collect::<Result<Vec<_>>>()?;
            inputs.insert(cell.id, ids);
        }
        fn visit(
            id: u64,
            inputs: &BTreeMap<u64, Vec<u64>>,
            marks: &mut BTreeMap<u64, u8>,
            order: &mut Vec<u64>,
        ) -> Result<()> {
            match marks.get(&id) {
                Some(1) => return Err(Error::DependencyCycle),
                Some(2) => return Ok(()),
                _ => {}
            }
            marks.insert(id, 1);
            for dependency in &inputs[&id] {
                visit(*dependency, inputs, marks, order)?;
            }
            marks.insert(id, 2);
            order.push(id);
            Ok(())
        }
        let mut order = Vec::new();
        let mut marks = BTreeMap::new();
        for id in inputs.keys() {
            visit(*id, &inputs, &mut marks, &mut order)?;
        }
        Ok(Self { inputs, order })
    }
    pub fn edges(&self) -> Vec<DependencyEdge> {
        self.inputs
            .iter()
            .flat_map(|(to, from)| {
                from.iter().map(|from| DependencyEdge {
                    from: *from,
                    to: *to,
                })
            })
            .collect()
    }
    pub fn downstream(&self, seeds: &BTreeSet<u64>) -> BTreeSet<u64> {
        let mut affected = seeds.clone();
        for id in &self.order {
            if self.inputs[id]
                .iter()
                .any(|dependency| affected.contains(dependency))
            {
                affected.insert(*id);
            }
        }
        affected
    }
}
/// References expand authored definitions, before domain-dependent simplification.
/// This lets surrounding integral/sum binders retain their original scope.
pub fn resolve(g: &mut Graph, branch: &Branch, root: Id) -> Result<Id> {
    let definitions: BTreeMap<_, _> = branch
        .cells
        .iter()
        .map(|c| (c.name.as_str(), c.root))
        .collect();
    fn go(
        g: &mut Graph,
        root: Id,
        definitions: &BTreeMap<&str, Id>,
        memo: &mut BTreeMap<Id, Id>,
        depth: usize,
    ) -> Result<Id> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        if let Some(id) = memo.get(&root) {
            return Ok(*id);
        }
        let node = g.nodes[root].clone();
        let result = if let Node::Reference(name) = &node {
            let target = definitions
                .get(name.as_str())
                .ok_or_else(|| Error::MissingReference(name.clone()))?;
            go(g, *target, definitions, memo, depth + 1)?
        } else {
            let children = node
                .children()
                .into_iter()
                .map(|id| go(g, id, definitions, memo, depth + 1))
                .collect::<Result<Vec<_>>>()?;
            let rebuilt = if matches!(node, Node::Pow(..))
                && matches!(&g.nodes[children[1]], Node::Symbol(name) if name == "T")
                && matches!(
                    structured::shape(g, children[0]),
                    Ok(structured::Shape::Matrix(..))
                ) {
                Node::Call("transpose".into(), children[0])
            } else {
                structured::rebuild(&node, &children)
            };
            g.intern(rebuilt)?
        };
        memo.insert(root, result);
        Ok(result)
    }
    go(g, root, &definitions, &mut BTreeMap::new(), 0)
}
pub fn resolve_current(p: &mut Project) -> Result<DependencyGraph> {
    let branch = p.current().clone();
    let dag = DependencyGraph::build(&p.graph, &branch)?;
    for id in &dag.order {
        let root = branch.cells.iter().find(|c| c.id == *id).unwrap().root;
        let expanded = resolve(&mut p.graph, &branch, root)?;
        p.current_mut()
            .cells
            .iter_mut()
            .find(|c| c.id == *id)
            .unwrap()
            .resolved_root = Some(expanded);
    }
    Ok(dag)
}
pub fn validate_plan(plan: &Calculation) -> Result<()> {
    if !graph::valid_symbol(&plan.variable)
        || plan.bindings.len() > 64
        || plan
            .bindings
            .iter()
            .any(|(name, value)| !graph::valid_symbol(name) || !value.is_finite())
        || ![
            "compute",
            "simplify",
            "differentiate",
            "integrate",
            "expand",
            "evaluate",
            "solve",
            "determinant",
            "inverse",
            "transpose",
        ]
        .contains(&plan.operation.as_str())
    {
        return Err(Error::Invalid("calculation recipe".into()));
    }
    Ok(())
}
pub fn recalculate(p: &mut Project, seeds: BTreeSet<u64>) -> Result<()> {
    let dag = resolve_current(p)?;
    let affected = dag.downstream(&seeds);
    let selected = p.current().selected;
    for id in dag.order.into_iter().filter(|id| affected.contains(id)) {
        let cell = p
            .current_mut()
            .cells
            .iter_mut()
            .find(|c| c.id == id)
            .unwrap();
        let plan = cell.calculation.clone().unwrap_or_default();
        cell.output = None;
        cell.calculation_error = None;
        cell.calculation = Some(plan.clone());
        if let Err(error) = crate::execute(
            p,
            &Command::Transform {
                id,
                operation: plan.operation,
                variable: plan.variable,
                bindings: plan.bindings,
            },
        ) {
            if matches!(error, Error::Limit) {
                return Err(error);
            }
            let revision = p.revision.checked_add(1).ok_or(Error::Limit)?;
            let cell = p
                .current_mut()
                .cells
                .iter_mut()
                .find(|c| c.id == id)
                .unwrap();
            cell.output = None;
            cell.calculation_error = Some(error.code().into());
            cell.calculated_revision = Some(revision);
        }
    }
    p.current_mut().selected = selected;
    Ok(())
}
pub fn assumption_users(p: &Project, symbol: &str) -> BTreeSet<u64> {
    p.current()
        .cells
        .iter()
        .filter(|cell| {
            let mut seen = BTreeSet::new();
            let mut todo = vec![cell.resolved_root.unwrap_or(cell.root)];
            while let Some(id) = todo.pop() {
                if seen.insert(id) {
                    if matches!(&p.graph.nodes[id], Node::Symbol(name) if name == symbol) {
                        return true;
                    }
                    todo.extend(p.graph.nodes[id].children());
                }
            }
            false
        })
        .map(|c| c.id)
        .collect()
}
/// Only exact reference tokens are renamed; ordinary variables are untouched.
pub fn rename_sources(p: &mut Project, old: &str, new: &str) -> Result<()> {
    if old == new {
        return Ok(());
    }
    let mut cells = p.current().cells.clone();
    for cell in &mut cells {
        if cell.source_format == SourceFormat::Latex {
            cell.source = cell
                .source
                .replace(&format!("\\cell{{{old}}}"), &format!("\\cell{{{new}}}"));
        } else {
            let source = cell.source.clone();
            let bytes = source.as_bytes();
            let mut i = 0;
            let mut output = String::new();
            let mut copied = 0;
            while i < bytes.len() {
                if bytes[i] == b'@' {
                    let start = i;
                    i += 1;
                    let name = i;
                    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
                    {
                        i += 1;
                    }
                    if &source[name..i] == old {
                        output.push_str(&source[copied..start]);
                        output.push('@');
                        output.push_str(new);
                        copied = i;
                    }
                } else {
                    i += 1;
                }
            }
            output.push_str(&source[copied..]);
            cell.source = output;
        }
        cell.root = cell.source_format.parse(&mut p.graph, &cell.source)?;
    }
    p.current_mut().cells = cells;
    Ok(())
}
