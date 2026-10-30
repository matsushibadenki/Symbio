//! Source-authoritative, read-only replay packages. Hashes detect changes, not authorship or proof.
use crate::{
    graph::{Graph, Id},
    *,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MAX_BYTES: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub format: String,
    pub version: u32,
    pub payload: Payload,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub project_id: uuid::Uuid,
    pub document: String,
    pub branch: String,
    pub revision: u64,
    pub engine: String,
    pub assumptions: BTreeMap<String, Assumption>,
    pub cells: Vec<RecipeCell>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeCell {
    pub id: u64,
    pub name: String,
    pub source: String,
    pub source_format: SourceFormat,
    pub note: String,
    pub calculation: Option<Calculation>,
    pub expected: Expected,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    pub output: Option<Evidence>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub expression_sha256: Option<String>,
    // Exact float64 bit comparison; no cross-platform tolerance or error guarantee is implied.
    pub numeric_bits: Option<String>,
    pub operation: String,
    pub trust: Trust,
    pub precision: String,
    pub conditions: Vec<String>,
    pub engine: String,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub document: String,
    pub branch: String,
    pub revision: u64,
    pub package_sha256: String,
    pub recorded_engine: String,
    pub current_engine: String,
    pub engine_matches: bool,
    pub reproduced: bool,
    pub matched: usize,
    pub errors_reproduced: usize,
    pub mismatched: usize,
    pub skipped: usize,
    pub cells: Vec<CellReport>,
}
#[derive(Debug, Serialize)]
pub struct CellReport {
    pub id: u64,
    pub name: String,
    pub status: &'static str,
    pub differences: Vec<&'static str>,
    pub expected: Expected,
    pub actual: Option<Expected>,
    pub result_mathml: Option<String>,
    pub result_text: Option<String>,
    pub result_preview_omitted: bool,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash_expression(g: &Graph, root: Id) -> Result<String> {
    fn go(g: &Graph, id: Id, memo: &mut BTreeMap<Id, String>, depth: usize) -> Result<String> {
        if depth > 96 {
            return Err(Error::Limit);
        }
        if let Some(hash) = memo.get(&id) {
            return Ok(hash.clone());
        }
        let node = g
            .nodes
            .get(id)
            .ok_or_else(|| Error::Invalid("result node".into()))?;
        let children = node.children();
        let hashes = children
            .iter()
            .map(|c| go(g, *c, memo, depth + 1))
            .collect::<Result<Vec<_>>>()?;
        let shape = structured::rebuild(node, &(0..children.len()).collect::<Vec<_>>());
        let hash = digest(&serde_json::to_vec(&(shape, hashes))?);
        memo.insert(id, hash.clone());
        Ok(hash)
    }
    go(g, root, &mut BTreeMap::new(), 0)
}
// Early sample documents used lowercase domain labels; normalize only known domain annotations.
fn condition(text: &str) -> String {
    let Some((symbol, value)) = text.split_once(": ") else {
        return text.into();
    };
    if !graph::valid_symbol(symbol) {
        return text.into();
    }
    let (domain, suffix) = value
        .strip_suffix(", nonzero")
        .map(|d| (d, ", nonzero"))
        .unwrap_or((value, ""));
    let canonical = match domain {
        "Unspecified" | "unspecified" => "unspecified",
        "Real" | "real" => "real",
        "Nonnegative" | "nonnegative" => "nonnegative",
        "Positive" | "positive" => "positive",
        "Complex" | "complex" => "complex",
        _ => return text.into(),
    };
    format!("{symbol}: {canonical}{suffix}")
}
// DAG sharing can make a tiny input expand exponentially when typeset.
fn preview_fits(g: &Graph, root: Id) -> bool {
    fn cost(g: &Graph, id: Id, memo: &mut BTreeMap<Id, usize>) -> usize {
        if let Some(n) = memo.get(&id) {
            return *n;
        }
        let node = &g.nodes[id];
        let mut n = 256
            + match node {
                graph::Node::Number(s)
                | graph::Node::Symbol(s)
                | graph::Node::Constant(s)
                | graph::Node::Reference(s) => s.len(),
                _ => 0,
            };
        for child in node.children() {
            n = n.saturating_add(cost(g, child, memo)).min(32769);
        }
        memo.insert(id, n);
        n
    }
    cost(g, root, &mut BTreeMap::new()) <= 32768
}
fn expected(g: &Graph, c: &Cell) -> Result<Expected> {
    Ok(Expected {
        error: c.calculation_error.clone(),
        output: c
            .output
            .as_ref()
            .map(|o| -> Result<Evidence> {
                Ok(Evidence {
                    expression_sha256: o.root.map(|r| hash_expression(g, r)).transpose()?,
                    numeric_bits: o.value.map(|v| format!("{:016x}", v.to_bits())),
                    operation: o.operation.clone(),
                    trust: o.trust.clone(),
                    precision: o.precision.clone(),
                    conditions: o.conditions.iter().map(|s| condition(s)).collect(),
                    engine: o.engine.clone(),
                })
            })
            .transpose()?,
    })
}
pub fn capture(p: &Project) -> Result<Package> {
    p.validate()?;
    let payload = Payload {
        project_id: p.id,
        document: p.name.clone(),
        branch: p.active.clone(),
        revision: p.revision,
        engine: ENGINE.into(),
        assumptions: p.current().assumptions.clone(),
        cells: p
            .current()
            .cells
            .iter()
            .map(|c| {
                Ok(RecipeCell {
                    id: c.id,
                    name: c.name.clone(),
                    source: c.source.clone(),
                    source_format: c.source_format.clone(),
                    note: c.note.clone(),
                    calculation: c.calculation.clone(),
                    expected: expected(&p.graph, c)?,
                })
            })
            .collect::<Result<_>>()?,
    };
    let sha256 = digest(&serde_json::to_vec(&payload)?);
    let package = Package {
        format: "symbio.reproduction".into(),
        version: 1,
        payload,
        sha256,
    };
    if serde_json::to_vec(&package)?.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    validate_header(&package)?;
    Ok(package)
}
pub fn to_bytes(p: &Package) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec_pretty(p)?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
pub fn from_bytes(bytes: &[u8]) -> Result<Package> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let p: Package = serde_json::from_slice(bytes)?;
    validate_header(&p)?;
    Ok(p)
}
fn validate_header(p: &Package) -> Result<()> {
    if p.version != 1 || p.format != "symbio.reproduction" {
        return Err(Error::Version);
    }
    if p.payload.cells.len() > 128
        || p.payload.assumptions.len() > 64
        || p.payload.engine.len() > 128
        || p.payload.branch.is_empty()
        || p.payload.branch.len() > 64
    {
        return Err(Error::Limit);
    }
    if serde_json::to_vec(p)?.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    if p.sha256 != digest(&serde_json::to_vec(&p.payload)?) {
        return Err(Error::Invalid("reproduction checksum".into()));
    }
    for cell in &p.payload.cells {
        if cell.source.len() > 2048 || cell.name.len() > 48 || cell.note.len() > 4096 {
            return Err(Error::Limit);
        }
        if cell.expected.error.as_ref().is_some_and(|s| s.len() > 128)
            || (cell.expected.output.is_some() && cell.expected.error.is_some())
        {
            return Err(Error::Invalid("reproduction expectation".into()));
        }
        if let Some(o) = &cell.expected.output {
            let hash_ok = |s: &str, len: usize| {
                s.len() == len
                    && s.bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            };
            if o.expression_sha256
                .as_ref()
                .is_some_and(|s| !hash_ok(s, 64))
                || o.numeric_bits.as_ref().is_some_and(|s| {
                    !hash_ok(s, 16)
                        || u64::from_str_radix(s, 16).is_ok_and(|v| !f64::from_bits(v).is_finite())
                })
                || o.conditions.len() > 128
                || o.conditions.iter().any(|s| s.len() > 512)
                || o.operation.len() > 64
                || o.engine.len() > 128
                || o.precision.len() > 128
            {
                return Err(Error::Invalid("reproduction evidence".into()));
            }
        }
    }
    Ok(())
}
fn fresh(p: &Payload) -> Result<Project> {
    let mut project = Project::empty(&p.document)?;
    project.id = p.project_id;
    let mut branch = project.branches.remove("main").expect("empty project");
    branch.name = p.branch.clone();
    branch.assumptions = p.assumptions.clone();
    for c in &p.cells {
        let root = c.source_format.parse(&mut project.graph, &c.source)?;
        branch.cells.push(Cell {
            id: c.id,
            name: c.name.clone(),
            source: c.source.clone(),
            source_format: c.source_format.clone(),
            root,
            output: None,
            calculation: c.calculation.clone(),
            calculation_error: Some("not_replayed".into()),
            calculated_revision: None,
            resolved_root: None,
            note: c.note.clone(),
        });
        project.next_cell = project
            .next_cell
            .max(c.id.checked_add(1).ok_or(Error::Limit)?);
    }
    project.active = p.branch.clone();
    project.branches.insert(p.branch.clone(), branch);
    project.validate()?;
    dependencies::resolve_current(&mut project)?;
    Ok(project)
}
pub fn verify(package: &Package) -> Result<Report> {
    validate_header(package)?;
    // No captured SEG/cache, filesystem path, command text or audit events are executed.
    let mut project = fresh(&package.payload)?;
    let order = dependencies::DependencyGraph::build(&project.graph, project.current())?.order;
    let mut rows = BTreeMap::new();
    let mut matched = 0;
    let mut errors_reproduced = 0;
    let mut mismatched = 0;
    let mut skipped = 0;
    for id in order {
        let original = package
            .payload
            .cells
            .iter()
            .find(|c| c.id == id)
            .expect("validated cell");
        let mut row = CellReport {
            id,
            name: original.name.clone(),
            status: "skipped",
            differences: vec![],
            expected: original.expected.clone(),
            actual: None,
            result_mathml: None,
            result_text: None,
            result_preview_omitted: false,
        };
        if let Some(plan) = &original.calculation
            && (original.expected.output.is_some() || original.expected.error.is_some())
        {
            let command = if plan.operation == "substitute" {
                Command::Substitute {
                    id,
                    values: plan.exact_bindings.clone(),
                }
            } else {
                Command::Transform {
                    id,
                    operation: plan.operation.clone(),
                    variable: plan.variable.clone(),
                    bindings: plan.bindings.clone(),
                }
            };
            let mut trial = project.clone();
            let actual = match crate::execute(&mut trial, &command) {
                Ok(()) => {
                    project = trial;
                    let cell = project.current().cells.iter().find(|c| c.id == id).unwrap();
                    row.result_text = cell
                        .output
                        .as_ref()
                        .and_then(|o| o.value.map(|v| v.to_string()));
                    row.result_preview_omitted = cell
                        .output
                        .as_ref()
                        .and_then(|o| o.root)
                        .is_some_and(|r| !preview_fits(&project.graph, r));
                    row.result_mathml = cell
                        .output
                        .as_ref()
                        .filter(|_| !row.result_preview_omitted)
                        .and_then(|o| {
                            o.root.map(|r| {
                                if o.trust == Trust::Approximate {
                                    project.graph.approximate_mathml(r)
                                } else {
                                    project.graph.mathml(r)
                                }
                            })
                        });
                    expected(&project.graph, cell)?
                }
                Err(error) => Expected {
                    output: None,
                    error: Some(error.code().into()),
                },
            };
            if original.expected.error != actual.error {
                row.differences.push("error");
            }
            match (&original.expected.output, &actual.output) {
                (Some(a), Some(b)) => {
                    if a.expression_sha256 != b.expression_sha256 {
                        row.differences.push("expression");
                    }
                    if a.numeric_bits != b.numeric_bits {
                        row.differences.push("numeric_value");
                    }
                    if a.operation != b.operation {
                        row.differences.push("operation");
                    }
                    if a.trust != b.trust
                        || a.precision != b.precision
                        || a.conditions != b.conditions
                    {
                        row.differences.push("evidence");
                    }
                    if a.engine != b.engine {
                        row.differences.push("engine");
                    }
                }
                (None, None) => {}
                _ => row.differences.push("output"),
            }
            if row.differences.is_empty() {
                if actual.error.is_some() {
                    row.status = "error_reproduced";
                    errors_reproduced += 1;
                } else {
                    row.status = "matched";
                    matched += 1;
                }
            } else {
                row.status = "mismatch";
                mismatched += 1;
            }
            row.actual = Some(actual);
        } else {
            row.differences.push(if original.calculation.is_none() {
                "missing_recipe"
            } else {
                "missing_expectation"
            });
            skipped += 1;
        }
        rows.insert(id, row);
    }
    Ok(Report {
        document: package.payload.document.clone(),
        branch: package.payload.branch.clone(),
        revision: package.payload.revision,
        package_sha256: package.sha256.clone(),
        recorded_engine: package.payload.engine.clone(),
        current_engine: ENGINE.into(),
        engine_matches: package.payload.engine == ENGINE,
        reproduced: !rows.is_empty()
            && skipped == 0
            && mismatched == 0
            && package.payload.engine == ENGINE,
        matched,
        errors_reproduced,
        mismatched,
        skipped,
        cells: package
            .payload
            .cells
            .iter()
            .map(|c| rows.remove(&c.id).unwrap())
            .collect(),
    })
}

pub fn read(path: impl AsRef<std::path::Path>) -> Result<Package> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    from_bytes(&bytes)
}
/// Never overwrites a research document, another package, or a symlink target.
pub fn write_new(path: impl AsRef<std::path::Path>, package: &Package) -> Result<()> {
    use std::io::Write;
    validate_header(package)?;
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&to_bytes(package)?)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path)
        .map_err(|e| Error::Io(e.error))?;
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}
