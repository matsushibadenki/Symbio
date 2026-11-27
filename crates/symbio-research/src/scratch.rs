//! Stateless tools shared by model providers and the MCP adapter.
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use symbio_core::{Actor, AppCore, Assumption, Command, ENGINE, Project, Request, SourceFormat};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preview {
    source: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Calculate {
    source: String,
    #[serde(default)]
    source_format: SourceFormat,
    #[serde(default = "compute")]
    operation: String,
    #[serde(default = "variable")]
    variable: String,
    #[serde(default)]
    assumptions: BTreeMap<String, Assumption>,
    #[serde(default)]
    bindings: BTreeMap<String, f64>,
    #[serde(default)]
    exact_bindings: BTreeMap<String, String>,
}
fn compute() -> String {
    "compute".into()
}
fn variable() -> String {
    "x".into()
}
fn apply(core: &mut AppCore, command: Command) -> symbio_core::Result<symbio_core::Snapshot> {
    core.apply(Request {
        expected_revision: core.project().revision,
        actor: Actor::Ai,
        command,
    })
}
fn calculate(args: Calculate) -> symbio_core::Result<Value> {
    let mut core = AppCore::new(Project::empty("MCP scratch")?)?;
    for (symbol, assumption) in args.assumptions {
        apply(&mut core, Command::Assume { symbol, assumption })?;
    }
    let define = match args.source_format {
        SourceFormat::Expression => Command::Define {
            id: None,
            name: "f".into(),
            source: args.source,
        },
        SourceFormat::Latex => Command::DefineLatex {
            id: None,
            name: "f".into(),
            source: args.source,
        },
    };
    apply(&mut core, define)?;
    let command = if args.operation == "substitute" {
        Command::Substitute {
            id: 1,
            values: args.exact_bindings,
        }
    } else {
        Command::Transform {
            id: 1,
            operation: args.operation,
            variable: args.variable,
            bindings: args.bindings,
        }
    };
    let cell = apply(&mut core, command)?
        .cells
        .into_iter()
        .next()
        .ok_or_else(|| symbio_core::Error::Invalid("missing scratch cell".into()))?;
    Ok(json!({"ok":true,"engine":ENGINE,"cell":cell,"seg":core.inspect(1)?}))
}
/// The requested tool or its arguments are outside the scratch API contract.
#[derive(Debug)]
pub struct InvalidToolCall;

/// Returns the structured kernel outcome, including any mathematical error.
pub fn execute(name: &str, arguments: Value) -> Result<Value, InvalidToolCall> {
    let computed = match name {
        "math_preview_latex" => {
            let args: Preview = serde_json::from_value(arguments).map_err(|_| InvalidToolCall)?;
            if args.source.is_empty() || args.source.len() > 2048 {
                return Err(InvalidToolCall);
            }
            symbio_core::latex::preview(&args.source).and_then(|p| {
                Ok(json!({"ok":true,"engine":ENGINE,"preview":serde_json::to_value(p)?}))
            })
        }
        "math_calculate" => {
            let args: Calculate = serde_json::from_value(arguments).map_err(|_| InvalidToolCall)?;
            if args.source.is_empty()
                || args.source.len() > 2048
                || args.assumptions.len() > 64
                || args.bindings.len() > 64
                || args.exact_bindings.len() > 64
                || (args.operation == "substitute") == args.exact_bindings.is_empty()
                || (!args.bindings.is_empty() && args.operation != "evaluate")
                || !symbio_core::graph::valid_symbol(&args.variable)
                || ![
                    "compute",
                    "simplify",
                    "differentiate",
                    "integrate",
                    "expand",
                    "solve",
                    "determinant",
                    "inverse",
                    "transpose",
                    "evaluate",
                    "substitute",
                ]
                .contains(&args.operation.as_str())
            {
                return Err(InvalidToolCall);
            }
            calculate(args)
        }
        _ => return Err(InvalidToolCall),
    };
    Ok(match computed {
        Ok(value) => value,
        Err(e) => {
            json!({"ok":false,"engine":ENGINE,"error":{"code":e.code(),"message":e.to_string()}})
        }
    })
}
pub fn schema() -> serde_json::Result<Value> {
    serde_json::from_str(include_str!("../../../docs/mcp-tools.json"))
}
