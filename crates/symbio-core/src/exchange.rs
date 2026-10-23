//! Bounded, GUI-independent bridge for source-authoritative document editors.
//! Results are derived data. Only an explicit export creates a research document.
use crate::*;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MathInput {
    pub source: String,
    pub operation: String,
    pub variable: String,
    #[serde(default)]
    pub bindings: BTreeMap<String, f64>,
    #[serde(default)]
    pub assumptions: BTreeMap<String, Assumption>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MathOrigin {
    pub document_id: uuid::Uuid,
    pub node_id: uuid::Uuid,
}
#[derive(Debug, Serialize)]
pub struct ImportedMath {
    pub source: String,
    pub name: String,
    pub assumptions: BTreeMap<String, Assumption>,
}

fn prepare(input: &MathInput) -> Result<AppCore> {
    if input.source.len() > 2048 || input.assumptions.len() > 64 || input.bindings.len() > 64 {
        return Err(Error::Limit);
    }
    let mut project = Project::new("KOMYAKU equation")?;
    project.graph = graph::Graph::default();
    project.current_mut().cells.clear();
    project.current_mut().selected = None;
    project.current_mut().assumptions = input.assumptions.clone();
    project.next_cell = 1;
    let mut core = AppCore::new(project)?;
    core.apply(Request {
        expected_revision: 0,
        actor: Actor::Human,
        command: Command::DefineLatex {
            id: None,
            name: "equation".into(),
            source: input.source.clone(),
        },
    })?;
    Ok(core)
}

/// Uses the exact same transactions as Symbio, without filesystem or network access.
pub fn compute(input: &MathInput) -> Result<CellView> {
    let mut core = prepare(input)?;
    let snapshot = core.apply(Request {
        expected_revision: core.project().revision,
        actor: Actor::Human,
        command: Command::Transform {
            id: 1,
            operation: input.operation.clone(),
            variable: input.variable.clone(),
            bindings: input.bindings.clone(),
        },
    })?;
    snapshot
        .cells
        .into_iter()
        .next()
        .ok_or_else(|| Error::Invalid("equation".into()))
}

/// Export only this equation and its explicit assumptions, never another branch or draft.
pub fn export(input: &MathInput, origin: Option<MathOrigin>) -> Result<String> {
    let mut core = prepare(input)?;
    if let Some(origin) = origin {
        core.apply(Request {
            expected_revision: core.project().revision,
            actor: Actor::Human,
            command: Command::Note {
                id: 1,
                text: serde_json::to_string(&origin)?,
            },
        })?;
    }
    Ok(serde_json::to_string_pretty(core.project())?)
}

/// Import only the selected source of the active branch. Stored outputs are never adopted.
pub fn import(json: &str) -> Result<ImportedMath> {
    if json.len() > 8 * 1024 * 1024 {
        return Err(Error::Limit);
    }
    let project: Project = serde_json::from_str(json)?;
    project.validate()?;
    let branch = project.current();
    let cell = branch
        .cells
        .iter()
        .find(|c| Some(c.id) == branch.selected)
        .ok_or_else(|| Error::Invalid("select an equation in Symbio before exporting".into()))?;
    let source = match cell.source_format {
        SourceFormat::Latex => cell.source.clone(),
        SourceFormat::Expression => {
            latex::convert(&cell.source, SourceFormat::Expression, SourceFormat::Latex)?
        }
    };
    // Preview also enforces the editor-facing input/shape limits after conversion.
    latex::preview(&source)?;
    Ok(ImportedMath {
        source,
        name: cell.name.clone(),
        assumptions: branch.assumptions.clone(),
    })
}
