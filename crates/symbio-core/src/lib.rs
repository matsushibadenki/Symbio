//! Symbio's UI-independent owner: all operations are revision-checked transactions.
pub mod algebra;
pub mod dependencies;
pub mod exchange;
pub mod graph;
pub mod latex;
pub mod model;
pub mod parser;
mod storage;
pub mod structured;
pub use model::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub use storage::ProjectStore;
pub const ENGINE: &str = "symbio-rust/0.3.0";
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("revision conflict")]
    Conflict,
    #[error("missing cell reference: {0}")]
    MissingReference(String),
    #[error("cyclic cell dependency")]
    DependencyCycle,
    #[error("unsupported project version")]
    Version,
    #[error("invalid project: {0}")]
    Invalid(String),
    #[error("parse error at token {0}")]
    Parse(usize),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
    #[error("domain error: {0}")]
    Domain(String),
    #[error("resource limit exceeded")]
    Limit,
    #[error("project locked")]
    Locked,
    #[error("external change detected")]
    ExternalChange,
    #[error("history empty")]
    HistoryEmpty,
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}
impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingReference(_) => "missing_reference",
            Self::DependencyCycle => "dependency_cycle",
            Self::Conflict => "revision_conflict",
            Self::Version => "unsupported_version",
            Self::Invalid(_) => "invalid_project",
            Self::Parse(_) => "parse_error",
            Self::Unsupported(_) => "unsupported",
            Self::Domain(_) => "domain_error",
            Self::Limit => "resource_limit",
            Self::Locked => "project_locked",
            Self::ExternalChange => "external_change",
            Self::HistoryEmpty => "history_empty",
            Self::Io(_) => "io_error",
            Self::Json(_) => "invalid_json",
        }
    }
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    #[serde(rename = "math.define")]
    Define {
        id: Option<u64>,
        name: String,
        source: String,
    },
    #[serde(rename = "math.define_latex")]
    DefineLatex {
        id: Option<u64>,
        name: String,
        source: String,
    },
    #[serde(rename = "math.assume")]
    Assume {
        symbol: String,
        assumption: Assumption,
    },
    #[serde(rename = "math.recalculate")]
    Recalculate { id: Option<u64> },
    #[serde(rename = "math.transform")]
    Transform {
        id: u64,
        operation: String,
        variable: String,
        #[serde(default)]
        bindings: BTreeMap<String, f64>,
    },
    #[serde(rename = "math.select")]
    Select { id: u64 },
    #[serde(rename = "math.remove")]
    Remove { id: u64 },
    #[serde(rename = "math.note")]
    Note { id: u64, text: String },
    #[serde(rename = "math.branch")]
    Branch { name: String },
    #[serde(rename = "math.checkout")]
    Checkout { name: String },
    #[serde(rename = "math.rename")]
    Rename { name: String },
    #[serde(rename = "math.undo")]
    Undo,
    #[serde(rename = "math.redo")]
    Redo,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub expected_revision: u64,
    pub actor: Actor,
    pub command: Command,
}
#[derive(Clone, Debug, Serialize)]
pub struct CellView {
    #[serde(flatten)]
    pub cell: Cell,
    pub expression: String,
    pub kind: String,
    pub mathml: String,
    pub result_text: Option<String>,
    pub result_mathml: Option<String>,
    pub symbols: Vec<String>,
    pub dimension: Option<[i32; 3]>,
    pub dependencies: Vec<u64>,
    pub dependents: Vec<u64>,
    pub resolved_expression: String,
    pub resolved_mathml: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub project_id: uuid::Uuid,
    pub name: String,
    pub revision: u64,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub active: String,
    pub branches: Vec<String>,
    pub selected: Option<u64>,
    pub cells: Vec<CellView>,
    pub assumptions: BTreeMap<String, Assumption>,
    pub history: Vec<Event>,
    pub node_count: usize,
    pub engine: String,
    pub dependencies: Vec<dependencies::DependencyEdge>,
    pub calculation_order: Vec<u64>,
}
pub struct AppCore {
    project: Project,
    undo: Vec<Project>,
    redo: Vec<Project>,
    saved: Vec<u8>,
}
impl AppCore {
    pub fn new(mut project: Project) -> Result<Self> {
        project.validate()?;
        project.graph.rebuild()?;
        // Recover legacy recipes only when the audit records their actual parameters.
        // Parameter-free operations can also be reconstructed from their output label.
        for (name, branch) in &mut project.branches {
            for cell in &mut branch.cells {
                if cell.calculation.is_some() {
                    continue;
                }
                let Some(output) = &cell.output else {
                    continue;
                };
                let recorded = project
                    .history
                    .iter()
                    .rev()
                    .filter(|event| event.branch == *name)
                    .filter_map(|event| {
                        serde_json::from_value::<Command>(event.command.clone()).ok()
                    })
                    .take_while(|command| !matches!(command, Command::Undo | Command::Redo))
                    .find_map(|command| match command {
                        Command::Transform {
                            id,
                            operation,
                            variable,
                            bindings,
                        } if id == cell.id && operation == output.operation => Some(Calculation {
                            operation,
                            variable,
                            bindings,
                        }),
                        _ => None,
                    });
                cell.calculation = recorded.or_else(|| {
                    ["compute", "simplify", "determinant", "inverse", "transpose"]
                        .contains(&output.operation.as_str())
                        .then(|| Calculation {
                            operation: output.operation.clone(),
                            ..Default::default()
                        })
                });
            }
        }
        let active = project.active.clone();
        for branch in project.branches.keys().cloned().collect::<Vec<_>>() {
            project.active = branch;
            dependencies::resolve_current(&mut project)?;
        }
        project.active = active;
        project.validate()?;
        let saved = serde_json::to_vec(&project)?;
        Ok(Self {
            project,
            undo: vec![],
            redo: vec![],
            saved,
        })
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn mark_saved(&mut self) {
        self.saved = serde_json::to_vec(&self.project).expect("serializable project");
    }
    pub fn dirty(&self) -> bool {
        serde_json::to_vec(&self.project).is_ok_and(|b| b != self.saved)
    }
    pub fn snapshot(&self) -> Snapshot {
        let p = &self.project;
        let b = p.current();
        let dag =
            dependencies::DependencyGraph::build(&p.graph, b).expect("validated dependencies");
        Snapshot {
            project_id: p.id,
            name: p.name.clone(),
            revision: p.revision,
            dirty: self.dirty(),
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            active: p.active.clone(),
            branches: p.branches.keys().cloned().collect(),
            selected: b.selected,
            cells: b
                .cells
                .iter()
                .map(|c| CellView {
                    cell: c.clone(),
                    kind: structured::kind(&p.graph, c.resolved_root.unwrap_or(c.root)).into(),
                    expression: p.graph.text(c.root),
                    mathml: p.graph.mathml(c.root),
                    result_text: c.output.as_ref().map(|o| {
                        o.root.map(|r| p.graph.text(r)).unwrap_or_else(|| {
                            o.value.map(|v| format!("{v:.12}")).unwrap_or_default()
                        })
                    }),
                    result_mathml: c.output.as_ref().and_then(|o| {
                        o.root.map(|r| {
                            if o.trust == Trust::Approximate {
                                p.graph.approximate_mathml(r)
                            } else {
                                p.graph.mathml(r)
                            }
                        })
                    }),
                    dependencies: dag.inputs[&c.id].clone(),
                    dependents: dag
                        .inputs
                        .iter()
                        .filter(|(_, deps)| deps.contains(&c.id))
                        .map(|(id, _)| *id)
                        .collect(),
                    resolved_expression: p.graph.text(c.resolved_root.unwrap_or(c.root)),
                    resolved_mathml: p.graph.mathml(c.resolved_root.unwrap_or(c.root)),
                    symbols: p
                        .graph
                        .symbols(c.resolved_root.unwrap_or(c.root))
                        .into_iter()
                        .collect(),
                    dimension: algebra::dimension(
                        &p.graph,
                        c.resolved_root.unwrap_or(c.root),
                        &b.assumptions,
                        0,
                    )
                    .ok()
                    .flatten(),
                })
                .collect(),
            assumptions: b.assumptions.clone(),
            history: p.history.iter().rev().take(32).cloned().collect(),
            node_count: p.graph.nodes.len(),
            engine: ENGINE.into(),
            dependencies: dag.edges(),
            calculation_order: dag.order,
        }
    }
    pub fn apply(&mut self, request: Request) -> Result<Snapshot> {
        if request.expected_revision != self.project.revision {
            return Err(Error::Conflict);
        }
        let mut next = self.project.clone();
        match &request.command {
            Command::Undo => {
                next = self.undo.last().cloned().ok_or(Error::HistoryEmpty)?;
            }
            Command::Redo => {
                next = self.redo.last().cloned().ok_or(Error::HistoryEmpty)?;
            }
            command => execute(&mut next, command)?,
        }
        next.revision = self.project.revision.checked_add(1).ok_or(Error::Limit)?;
        // Audit log records actual operations, including history navigation.
        next.history = self.project.history.clone();
        next.history.push(Event {
            revision: next.revision,
            actor: request.actor,
            branch: next.active.clone(),
            command: serde_json::to_value(&request.command)?,
            engine: ENGINE.into(),
        });
        if next.history.len() > 256 {
            next.history.remove(0);
        }
        next.validate()?;
        match request.command {
            Command::Undo => {
                self.undo.pop();
                self.redo.push(self.project.clone());
            }
            Command::Redo => {
                self.redo.pop();
                self.undo.push(self.project.clone());
            }
            _ => {
                self.undo.push(self.project.clone());
                self.redo.clear();
            }
        }
        if self.undo.len() > 32 {
            self.undo.remove(0);
        }
        self.project = next;
        Ok(self.snapshot())
    }
    pub fn preview_latex(&self, source: &str, id: Option<u64>) -> Result<latex::Preview> {
        let mut graph = self.project.graph.clone();
        let root = latex::parse(&mut graph, source)?;
        let mut branch = self.project.current().clone();
        if let Some(id) = id {
            let cell = branch
                .cells
                .iter_mut()
                .find(|c| c.id == id)
                .ok_or_else(|| Error::Invalid("cell".into()))?;
            cell.root = root;
        }
        dependencies::DependencyGraph::build(&graph, &branch)?;
        let resolved = dependencies::resolve(&mut graph, &branch, root)?;
        structured::shape(&graph, resolved)?;
        Ok(latex::Preview {
            expression: graph.text(root),
            mathml: graph.mathml(root),
            symbols: graph.symbols(resolved).into_iter().collect(),
            kind: structured::kind(&graph, resolved).into(),
        })
    }
    pub fn inspect(&self, id: u64) -> Result<serde_json::Value> {
        let cell = self
            .project
            .current()
            .cells
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| Error::Invalid("cell".into()))?;
        let mut seen = std::collections::BTreeSet::new();
        let mut pending = vec![cell.root];
        pending.extend(cell.resolved_root);
        if let Some(root) = cell.output.as_ref().and_then(|o| o.root) {
            pending.push(root)
        }
        while let Some(i) = pending.pop() {
            if seen.insert(i) {
                pending.extend(self.project.graph.nodes[i].children())
            }
        }
        Ok(
            serde_json::json!({"cell":cell,"nodes":seen.into_iter().map(|i|serde_json::json!({"id":i,"node":self.project.graph.nodes[i],"children":self.project.graph.nodes[i].children()})).collect::<Vec<_>>(),"assumptions":self.project.current().assumptions,"engine":ENGINE,"revision":self.project.revision}),
        )
    }
}
fn execute(p: &mut Project, command: &Command) -> Result<()> {
    match command {
        Command::Define { id, name, source } | Command::DefineLatex { id, name, source } => {
            let source_format = if matches!(command, Command::DefineLatex { .. }) {
                SourceFormat::Latex
            } else {
                SourceFormat::Expression
            };
            if !graph::valid_symbol(name) {
                return Err(Error::Invalid("name".into()));
            }
            let root = source_format.parse(&mut p.graph, source)?;
            if dependencies::references(&p.graph, root).is_empty() {
                structured::shape(&p.graph, root)?;
                algebra::dimension(&p.graph, root, &p.current().assumptions, 0)?;
            }
            if p.current()
                .cells
                .iter()
                .any(|c| c.name == *name && Some(c.id) != *id)
            {
                return Err(Error::Invalid("duplicate name".into()));
            }
            let mut changed_id = p.next_cell;
            if let Some(id) = id {
                changed_id = *id;
                let old = p
                    .current()
                    .cells
                    .iter()
                    .find(|c| c.id == *id)
                    .ok_or_else(|| Error::Invalid("cell".into()))?
                    .name
                    .clone();
                dependencies::rename_sources(p, &old, name)?;
                let c = p
                    .current_mut()
                    .cells
                    .iter_mut()
                    .find(|c| c.id == *id)
                    .ok_or_else(|| Error::Invalid("cell".into()))?;
                c.name = name.clone();
                c.source = source.clone();
                c.source_format = source_format;
                c.root = root;
                c.output = None;
            } else {
                let id = p.next_cell;
                p.next_cell = p.next_cell.checked_add(1).ok_or(Error::Limit)?;
                p.current_mut().cells.push(Cell {
                    id,
                    name: name.clone(),
                    source: source.clone(),
                    source_format,
                    root,
                    output: None,
                    calculation: Some(Calculation::default()),
                    calculation_error: None,
                    calculated_revision: None,
                    resolved_root: None,
                    note: String::new(),
                });
                p.current_mut().selected = Some(id);
            }
            dependencies::recalculate(p, std::collections::BTreeSet::from([changed_id]))?;
        }
        Command::Assume { symbol, assumption } => {
            if !graph::valid_symbol(symbol) {
                return Err(Error::Invalid("symbol".into()));
            }
            p.current_mut()
                .assumptions
                .insert(symbol.clone(), assumption.clone());
            let affected = dependencies::assumption_users(p, symbol);
            dependencies::recalculate(p, affected)?;
        }
        Command::Recalculate { id } => {
            let seeds = if let Some(id) = id {
                if !p.current().cells.iter().any(|c| c.id == *id) {
                    return Err(Error::Invalid("cell".into()));
                }
                std::collections::BTreeSet::from([*id])
            } else {
                p.current().cells.iter().map(|c| c.id).collect()
            };
            dependencies::recalculate(p, seeds)?;
        }
        Command::Transform {
            id,
            operation,
            variable,
            bindings,
        } => {
            if !graph::valid_symbol(variable) {
                return Err(Error::Invalid("variable".into()));
            }
            dependencies::resolve_current(p)?;
            let root = p
                .current()
                .cells
                .iter()
                .find(|c| c.id == *id)
                .ok_or_else(|| Error::Invalid("cell".into()))?
                .resolved_root
                .unwrap();
            structured::shape(&p.graph, root)?;
            algebra::dimension(&p.graph, root, &p.current().assumptions, 0)?;
            let assumptions = p.current().assumptions.clone();
            let mut conditions: Vec<String> = assumptions
                .iter()
                .map(|(s, a)| {
                    format!(
                        "{s}: {:?}{}",
                        a.domain,
                        if a.nonzero { ", nonzero" } else { "" }
                    )
                })
                .collect();
            let (result, value, trust, precision) = match operation.as_str() {
                "solve" => {
                    conditions.push("real roots; symbolic solver, not a formal proof".into());
                    (
                        Some(structured::solve(
                            &mut p.graph,
                            root,
                            variable,
                            &assumptions,
                        )?),
                        None,
                        Trust::Symbolic,
                        "exact",
                    )
                }
                "determinant" | "inverse" | "transpose" => {
                    let r = algebra::simplify(&mut p.graph, root, &assumptions)?;
                    (
                        Some(structured::matrix_call(
                            &mut p.graph,
                            if operation == "determinant" {
                                "det"
                            } else {
                                operation
                            },
                            r,
                            &assumptions,
                        )?),
                        None,
                        Trust::Symbolic,
                        "exact",
                    )
                }
                "compute" | "simplify" => (
                    Some(algebra::simplify(&mut p.graph, root, &assumptions)?),
                    None,
                    Trust::Symbolic,
                    "exact",
                ),
                "differentiate" => (
                    Some(algebra::differentiate(
                        &mut p.graph,
                        root,
                        variable,
                        &assumptions,
                    )?),
                    None,
                    Trust::Symbolic,
                    "exact",
                ),
                "integrate" => {
                    if !matches!(
                        p.graph.nodes[root],
                        graph::Node::Integral { lower: Some(_), .. }
                    ) {
                        conditions.push("antiderivative; add arbitrary constant C".into());
                    }
                    (
                        Some(
                            if matches!(p.graph.nodes[root], graph::Node::Integral { .. }) {
                                algebra::simplify(&mut p.graph, root, &assumptions)?
                            } else {
                                algebra::integrate(&mut p.graph, root, variable)?
                            },
                        ),
                        None,
                        Trust::Symbolic,
                        "exact",
                    )
                }
                "expand" => {
                    let poly = algebra::polynomial(&p.graph, root, variable, 0)?;
                    (
                        Some(algebra::from_polynomial(&mut p.graph, &poly, variable)?),
                        None,
                        Trust::Symbolic,
                        "exact",
                    )
                }
                "evaluate" => {
                    if bindings.len() > 64 || bindings.values().any(|v| !v.is_finite()) {
                        return Err(Error::Invalid("bindings".into()));
                    }
                    for (s, a) in &assumptions {
                        if let Some(v) = bindings.get(s)
                            && ((a.nonzero && *v == 0.)
                                || (a.domain == Domain::Positive && *v <= 0.)
                                || (a.domain == Domain::Nonnegative && *v < 0.))
                        {
                            return Err(Error::Domain("binding violates assumption".into()));
                        }
                    }
                    conditions.extend(bindings.iter().map(|(s, v)| format!("{s} = {v}")));
                    let (result, value) =
                        structured::numeric(&mut p.graph, root, bindings, &assumptions)?;
                    (
                        result,
                        value,
                        Trust::Approximate,
                        "float64; no certified error bound",
                    )
                }
                _ => return Err(Error::Unsupported(operation.clone())),
            };
            if matches!(operation.as_str(), "compute" | "simplify" | "integrate")
                && structured::has_indefinite_integral(&p.graph, root)
                && !conditions
                    .iter()
                    .any(|s| s == "antiderivative; add arbitrary constant C")
            {
                conditions.push("antiderivative; add arbitrary constant C".into());
            }
            let output = Output {
                root: result,
                value,
                operation: operation.clone(),
                trust,
                precision: precision.into(),
                conditions,
                engine: ENGINE.into(),
            };
            let revision = p.revision.checked_add(1).ok_or(Error::Limit)?;
            let c = p
                .current_mut()
                .cells
                .iter_mut()
                .find(|c| c.id == *id)
                .unwrap();
            c.output = Some(output);
            c.calculation_error = None;
            c.calculated_revision = Some(revision);
            c.calculation = Some(Calculation {
                operation: operation.clone(),
                variable: variable.clone(),
                bindings: bindings.clone(),
            });
            p.current_mut().selected = Some(*id);
        }
        Command::Select { id } => {
            if !p.current().cells.iter().any(|c| c.id == *id) {
                return Err(Error::Invalid("cell".into()));
            }
            p.current_mut().selected = Some(*id)
        }
        Command::Remove { id } => {
            let b = p.current_mut();
            if !b.cells.iter().any(|c| c.id == *id) {
                return Err(Error::Invalid("cell".into()));
            }
            b.cells.retain(|c| c.id != *id);
            if b.selected == Some(*id) {
                b.selected = b.cells.first().map(|c| c.id)
            }
        }
        Command::Note { id, text } => {
            p.current_mut()
                .cells
                .iter_mut()
                .find(|c| c.id == *id)
                .ok_or_else(|| Error::Invalid("cell".into()))?
                .note = text.clone()
        }
        Command::Branch { name } => {
            if name.trim().is_empty() || name.len() > 64 || p.branches.contains_key(name) {
                return Err(Error::Invalid("branch name".into()));
            }
            let mut b = p.current().clone();
            b.parent = Some(p.active.clone());
            b.name = name.clone();
            p.branches.insert(name.clone(), b);
            p.active = name.clone();
        }
        Command::Checkout { name } => {
            if !p.branches.contains_key(name) {
                return Err(Error::Invalid("branch".into()));
            }
            p.active = name.clone()
        }
        Command::Rename { name } => p.name = name.clone(),
        Command::Undo | Command::Redo => unreachable!(),
    }
    Ok(())
}
