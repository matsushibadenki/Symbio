//! Bounded mark / rebuild collection. Cell IDs are stable; SEG IDs belong to a revision.
use crate::{
    graph::{Graph, Id},
    *,
};
#[derive(Clone, Debug, Serialize)]
pub struct GraphStats {
    pub total: usize,
    pub live: usize,
    pub unused: usize,
}
fn reachable(p: &Project) -> Result<Vec<bool>> {
    let mut marked = vec![false; p.graph.nodes.len()];
    let mut pending = Vec::new();
    for b in p.branches.values() {
        for c in &b.cells {
            pending.push(c.root);
            pending.extend(c.resolved_root);
            pending.extend(c.output.as_ref().and_then(|o| o.root));
        }
    }
    while let Some(id) = pending.pop() {
        let node = p
            .graph
            .nodes
            .get(id)
            .ok_or_else(|| Error::Invalid("collection root".into()))?;
        if marked[id] {
            continue;
        }
        marked[id] = true;
        for child in node.children() {
            if child >= id {
                return Err(Error::Invalid("DAG reference".into()));
            }
            pending.push(child);
        }
    }
    Ok(marked)
}
pub fn stats(p: &Project) -> Result<GraphStats> {
    let total = p.graph.nodes.len();
    let live = reachable(p)?.iter().filter(|v| **v).count();
    Ok(GraphStats {
        total,
        live,
        unused: total - live,
    })
}
/// Build and validate a replacement before publishing any IDs. Undo snapshots own their graphs.
pub fn compact(p: &mut Project) -> Result<usize> {
    p.validate()?;
    let marked = reachable(p)?;
    let before = p.graph.nodes.len();
    let mut graph = Graph::default();
    let mut map = vec![None; before];
    for (id, node) in p.graph.nodes.iter().enumerate() {
        if marked[id] {
            let children = node
                .children()
                .iter()
                .map(|id| map[*id].ok_or_else(|| Error::Invalid("collection child".into())))
                .collect::<Result<Vec<Id>>>()?;
            map[id] = Some(graph.intern(structured::rebuild(node, &children))?);
        }
    }
    let mut next = p.clone();
    next.graph = graph;
    let mapped = |id: Id| {
        map.get(id)
            .and_then(|id| *id)
            .ok_or_else(|| Error::Invalid("collection root".into()))
    };
    for branch in next.branches.values_mut() {
        for cell in &mut branch.cells {
            cell.root = mapped(cell.root)?;
            cell.resolved_root = cell.resolved_root.map(mapped).transpose()?;
            if let Some(output) = &mut cell.output {
                output.root = output.root.map(mapped).transpose()?;
            }
        }
    }
    next.validate()?;
    let removed = before - next.graph.nodes.len();
    *p = next;
    Ok(removed)
}
