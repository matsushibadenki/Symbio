use crate::{
    Error, Result, algebra,
    graph::{Graph, Id, valid_symbol},
    parser,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    #[default]
    Unspecified,
    Real,
    Nonnegative,
    Positive,
    Complex,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Assumption {
    pub domain: Domain,
    #[serde(default)]
    pub nonzero: bool,
    #[serde(default)]
    pub dimension: Option<[i32; 3]>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    Symbolic,
    Approximate,
    Conjecture,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub root: Option<Id>,
    pub value: Option<f64>,
    pub operation: String,
    pub trust: Trust,
    pub precision: String,
    pub conditions: Vec<String>,
    pub engine: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    #[default]
    Expression,
    Latex,
}
impl SourceFormat {
    pub fn parse(&self, graph: &mut Graph, source: &str) -> Result<Id> {
        match self {
            Self::Expression => parser::parse(graph, source),
            Self::Latex => crate::latex::parse(graph, source),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Calculation {
    pub operation: String,
    pub variable: String,
    #[serde(default)]
    pub bindings: BTreeMap<String, f64>,
}
impl Default for Calculation {
    fn default() -> Self {
        Self {
            operation: "compute".into(),
            variable: "x".into(),
            bindings: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub id: u64,
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub source_format: SourceFormat,
    pub root: Id,
    pub output: Option<Output>,
    #[serde(default)]
    pub calculation: Option<Calculation>,
    #[serde(default)]
    pub calculation_error: Option<String>,
    #[serde(default)]
    pub calculated_revision: Option<u64>,
    #[serde(default)]
    pub resolved_root: Option<Id>,
    pub note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Branch {
    pub name: String,
    pub parent: Option<String>,
    pub cells: Vec<Cell>,
    pub assumptions: BTreeMap<String, Assumption>,
    pub selected: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub revision: u64,
    pub actor: Actor,
    pub branch: String,
    pub command: serde_json::Value,
    pub engine: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    Human,
    Ai,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub schema_version: u32,
    pub id: uuid::Uuid,
    pub name: String,
    pub revision: u64,
    pub next_cell: u64,
    pub graph: Graph,
    pub branches: BTreeMap<String, Branch>,
    pub active: String,
    pub history: Vec<Event>,
}
impl Project {
    pub fn new(name: &str) -> Result<Self> {
        let mut graph = Graph::default();
        let root = parser::parse(&mut graph, "x^3 + 2*x^2 - 5*x")?;
        let second = parser::parse(&mut graph, "sqrt(x^2)")?;
        let assumptions = BTreeMap::from([(
            "x".into(),
            Assumption {
                domain: Domain::Real,
                ..Default::default()
            },
        )]);
        let derivative = algebra::differentiate(&mut graph, root, "x", &assumptions)?;
        let simplified = algebra::simplify(&mut graph, second, &assumptions)?;
        let output = |root, operation: &str| {
            Some(Output {
                root: Some(root),
                value: None,
                operation: operation.into(),
                trust: Trust::Symbolic,
                precision: "exact".into(),
                conditions: vec!["x: real".into()],
                engine: crate::ENGINE.into(),
            })
        };
        let cells = vec![
            Cell {
                source_format: SourceFormat::Expression,
                id: 1,
                name: "f".into(),
                source: "x^3 + 2*x^2 - 5*x".into(),
                root,
                output: output(derivative, "differentiate"),
                calculation: Some(Calculation {
                    operation: "differentiate".into(),
                    ..Default::default()
                }),
                calculation_error: None,
                calculated_revision: None,
                resolved_root: None,
                note: String::new(),
            },
            Cell {
                source_format: SourceFormat::Expression,
                id: 2,
                name: "g".into(),
                source: "sqrt(x^2)".into(),
                root: second,
                output: output(simplified, "simplify"),
                calculation: Some(Calculation {
                    operation: "simplify".into(),
                    ..Default::default()
                }),
                calculation_error: None,
                calculated_revision: None,
                resolved_root: None,
                note: String::new(),
            },
        ];
        let branch = Branch {
            name: "main".into(),
            parent: None,
            cells,
            assumptions,
            selected: Some(1),
        };
        let project = Self {
            schema_version: 1,
            id: uuid::Uuid::new_v4(),
            name: name.into(),
            revision: 0,
            next_cell: 3,
            graph,
            branches: BTreeMap::from([("main".into(), branch)]),
            active: "main".into(),
            history: vec![],
        };
        project.validate()?;
        Ok(project)
    }
    pub fn current(&self) -> &Branch {
        &self.branches[&self.active]
    }
    pub fn current_mut(&mut self) -> &mut Branch {
        self.branches
            .get_mut(&self.active)
            .expect("validated branch")
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err(Error::Version);
        }
        if self.name.trim().is_empty()
            || self.name.len() > 120
            || self.branches.is_empty()
            || self.branches.len() > 16
            || !self.branches.contains_key(&self.active)
            || self.history.len() > 256
        {
            return Err(Error::Invalid("document".into()));
        }
        let mut graph = self.graph.clone();
        graph.rebuild()?;
        for (name, b) in &self.branches {
            if name != &b.name
                || name.is_empty()
                || name.len() > 64
                || b.cells.len() > 128
                || b.assumptions.len() > 64
            {
                return Err(Error::Limit);
            }
            let mut seen = BTreeSet::new();
            let mut names = BTreeSet::new();
            crate::dependencies::DependencyGraph::build(&graph, b)?;
            for c in &b.cells {
                if c.id >= self.next_cell
                    || !seen.insert(c.id)
                    || !names.insert(c.name.clone())
                    || !valid_symbol(&c.name)
                    || c.source.len() > 2048
                    || c.note.len() > 4096
                    || c.root >= graph.nodes.len()
                {
                    return Err(Error::Invalid("cell".into()));
                }
                if crate::dependencies::references(&graph, c.root).is_empty() {
                    crate::structured::shape(&graph, c.root)?;
                }
                if c.source_format.parse(&mut graph, &c.source)? != c.root {
                    return Err(Error::Invalid("source/root mismatch".into()));
                }
                if c.calculation_error.is_none()
                    && crate::dependencies::references(&graph, c.root).is_empty()
                {
                    algebra::dimension(&graph, c.root, &b.assumptions, 0)?;
                }
                if c.calculation_error.is_some() && c.output.is_some() {
                    return Err(Error::Invalid("failed calculation has output".into()));
                }
                if let Some(plan) = &c.calculation {
                    crate::dependencies::validate_plan(plan)?;
                }
                if let Some(root) = c.resolved_root {
                    if root >= self.graph.nodes.len() {
                        return Err(Error::Invalid("resolved root".into()));
                    }
                    let expected = crate::dependencies::resolve(&mut graph, b, c.root)?;
                    if root != expected {
                        return Err(Error::Invalid("resolved source mismatch".into()));
                    }
                    if c.calculation_error.is_none() {
                        crate::structured::shape(&graph, root)?;
                        algebra::dimension(&graph, root, &b.assumptions, 0)?;
                    }
                }
                if let Some(o) = &c.output
                    && (o.root.is_some_and(|r| r >= self.graph.nodes.len())
                        || o.value.is_some_and(|v| !v.is_finite())
                        || o.conditions.len() > 64
                        || o.conditions.iter().any(|s| s.len() > 256))
                {
                    return Err(Error::Invalid("output".into()));
                }
                if let Some(r) = c.output.as_ref().and_then(|o| o.root) {
                    crate::structured::shape(&graph, r)?;
                }
            }
            if b.selected.is_some_and(|id| !seen.contains(&id)) {
                return Err(Error::Invalid("selection".into()));
            }
            for (s, a) in &b.assumptions {
                if !valid_symbol(s)
                    || a.dimension
                        .is_some_and(|d| d.iter().any(|v| v.unsigned_abs() > 16))
                {
                    return Err(Error::Invalid("assumption".into()));
                }
            }
        }
        Ok(())
    }
}
