//! Strict MathJSON subset. Context envelopes retain assumptions; bare expressions have none.
use crate::{
    graph::{Graph, Id, Node},
    *,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub const MAX_BYTES: usize = 1024 * 1024;
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exchange {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub expression: Value,
    pub assumptions: BTreeMap<String, Assumption>,
}
fn unsupported() -> Error {
    Error::Unsupported("MathJSON subset".into())
}
fn reserved(s: &str) -> bool {
    [
        "Pi",
        "ExponentialE",
        "ImaginaryUnit",
        "Infinity",
        "PositiveInfinity",
        "NegativeInfinity",
        "ComplexInfinity",
        "NaN",
        "True",
        "False",
        "Nothing",
        "Add",
        "Subtract",
        "Multiply",
        "Divide",
        "Power",
        "Rational",
        "Negate",
        "Equal",
        "List",
        "Sin",
        "Cos",
        "Exp",
        "Ln",
        "Log",
        "Sqrt",
        "Abs",
        "Floor",
        "Ceil",
    ]
    .contains(&s)
}
// Multi-letter capitalized names may denote unsupported standard-library objects.
fn user_symbol(s: &str) -> bool {
    graph::valid_symbol(s) && !reserved(s) && !(s.len() > 1 && s.as_bytes()[0].is_ascii_uppercase())
}
fn integer(s: &str) -> Result<()> {
    let digits = s.strip_prefix('-').unwrap_or(s);
    if digits.is_empty() || digits.len() > 4096 || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return Err(unsupported());
    }
    Ok(())
}
fn head(v: &Value) -> Option<&[Value]> {
    v.as_array().map(Vec::as_slice).or_else(|| {
        v.as_object()
            .filter(|o| o.len() == 1)
            .and_then(|o| o.get("fn"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
    })
}
pub fn decode(g: &mut Graph, v: &Value) -> Result<Id> {
    fn go(g: &mut Graph, v: &Value, depth: usize, budget: &mut usize) -> Result<Id> {
        if depth > 48 || *budget == 0 {
            return Err(Error::Limit);
        }
        *budget -= 1;
        if let Some(n) = v.as_number() {
            if !(n.is_i64() || n.is_u64()) {
                return Err(unsupported());
            }
            return g.rat(graph::rational(&n.to_string())?);
        }
        if let Some(s) = v.as_str() {
            if s.starts_with('-') || s.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                integer(s)?;
                return g.rat(graph::rational(s)?);
            }
            return match s {
                "Pi" => g.intern(Node::Constant("pi".into())),
                "ExponentialE" => g.intern(Node::Constant("e".into())),
                _ if user_symbol(s) => g.intern(Node::Symbol(s.into())),
                _ => Err(unsupported()),
            };
        }
        if let Some(o) = v.as_object() {
            if o.len() != 1 {
                return Err(unsupported());
            }
            if let Some(n) = o.get("num") {
                let s = n.as_str().ok_or_else(unsupported)?;
                integer(s)?;
                return g.rat(graph::rational(s)?);
            }
            if let Some(s) = o.get("sym") {
                if !s.as_str().is_some_and(graph::valid_symbol) {
                    return Err(unsupported());
                }
                return go(g, s, depth + 1, budget);
            }
        }
        let a = head(v).ok_or_else(unsupported)?;
        let f = a.first().and_then(Value::as_str).ok_or_else(unsupported)?;
        let args = &a[1..];
        if f == "List" {
            if args.is_empty() || args.len() > 8 {
                return Err(unsupported());
            }
            let rows = args.len();
            let mut cols = None;
            let mut entries = Vec::new();
            for row in args {
                let row = head(row)
                    .filter(|r| r.first().and_then(Value::as_str) == Some("List"))
                    .ok_or_else(unsupported)?;
                let values = &row[1..];
                if values.is_empty() || values.len() > 8 || cols.is_some_and(|n| n != values.len())
                {
                    return Err(unsupported());
                }
                cols = Some(values.len());
                for x in values {
                    entries.push(go(g, x, depth + 1, budget)?);
                }
            }
            return g.intern(Node::Matrix {
                rows,
                cols: cols.unwrap(),
                entries,
            });
        }
        if ["Add", "Multiply"].contains(&f) {
            if args.len() < 2 || args.len() > 128 {
                return Err(unsupported());
            }
            let mut root = go(g, &args[0], depth + 1, budget)?;
            for v in &args[1..] {
                let x = go(g, v, depth + 1, budget)?;
                root = g.intern(if f == "Add" {
                    Node::Add(root, x)
                } else {
                    Node::Mul(root, x)
                })?;
            }
            return Ok(root);
        }
        let binary = matches!(f, "Subtract" | "Divide" | "Power" | "Equal" | "Rational");
        let unary = matches!(
            f,
            "Negate" | "Sin" | "Cos" | "Exp" | "Ln" | "Sqrt" | "Abs" | "Floor" | "Ceil"
        );
        if (!binary && !unary) || args.len() != if binary { 2 } else { 1 } {
            return Err(unsupported());
        }
        let x = go(g, &args[0], depth + 1, budget)?;
        if binary {
            let y = go(g, &args[1], depth + 1, budget)?;
            return if f == "Rational" {
                let n = g
                    .value(x)
                    .filter(|n| n.is_integer())
                    .ok_or_else(unsupported)?;
                let d = g
                    .value(y)
                    .filter(|d| d.is_integer())
                    .ok_or_else(unsupported)?;
                if d == graph::Rational::from_integer(0.into()) {
                    return Err(Error::Domain("division by zero".into()));
                }
                g.rat(n / d)
            } else {
                g.intern(match f {
                    "Subtract" => Node::Sub(x, y),
                    "Divide" => Node::Div(x, y),
                    "Power" => Node::Pow(x, y),
                    _ => Node::Equation(x, y),
                })
            };
        }
        if f == "Negate" {
            let zero = g.number(0)?;
            return g.intern(Node::Sub(zero, x));
        }
        g.intern(Node::Call(
            match f {
                "Sin" => "sin",
                "Cos" => "cos",
                "Exp" => "exp",
                "Ln" => "log",
                "Sqrt" => "sqrt",
                "Abs" => "abs",
                "Floor" => "floor",
                _ => "ceil",
            }
            .into(),
            x,
        ))
    }
    go(g, v, 0, &mut 4096)
}
pub fn encode(g: &Graph, root: Id) -> Result<Value> {
    fn go(g: &Graph, id: Id, depth: usize, budget: &mut usize) -> Result<Value> {
        if depth > 48 || *budget == 0 {
            return Err(Error::Limit);
        }
        *budget -= 1;
        Ok(match &g.nodes[id] {
            Node::Number(s) => {
                let (n, d) = s.split_once('/').unwrap_or((s, "1"));
                if d == "1" {
                    json!({"num":n})
                } else {
                    json!(["Rational",{"num":n},{"num":d}])
                }
            }
            Node::Symbol(s) if user_symbol(s) => json!(s),
            Node::Constant(s) => json!(if s == "pi" { "Pi" } else { "ExponentialE" }),
            Node::Add(x, y)
            | Node::Sub(x, y)
            | Node::Mul(x, y)
            | Node::Div(x, y)
            | Node::Pow(x, y)
            | Node::Equation(x, y) => {
                let f = match &g.nodes[id] {
                    Node::Add(..) => "Add",
                    Node::Sub(..) => "Subtract",
                    Node::Mul(..) => "Multiply",
                    Node::Div(..) => "Divide",
                    Node::Pow(..) => "Power",
                    _ => "Equal",
                };
                json!([
                    f,
                    go(g, *x, depth + 1, budget)?,
                    go(g, *y, depth + 1, budget)?
                ])
            }
            Node::Call(f, x) => {
                let f = match f.as_str() {
                    "sin" => "Sin",
                    "cos" => "Cos",
                    "exp" => "Exp",
                    "log" => "Ln",
                    "sqrt" => "Sqrt",
                    "abs" => "Abs",
                    "floor" => "Floor",
                    "ceil" => "Ceil",
                    _ => return Err(unsupported()),
                };
                json!([f, go(g, *x, depth + 1, budget)?])
            }
            Node::Matrix {
                rows,
                cols,
                entries,
            } => {
                let mut array = vec![json!("List")];
                for row in 0..*rows {
                    let mut r = vec![json!("List")];
                    for col in 0..*cols {
                        r.push(go(g, entries[row * cols + col], depth + 1, budget)?);
                    }
                    array.push(Value::Array(r));
                }
                Value::Array(array)
            }
            _ => return Err(unsupported()),
        })
    }
    go(g, root, 0, &mut 4096)
}
pub fn export(p: &Project, id: u64) -> Result<Exchange> {
    p.validate()?;
    let c = p
        .current()
        .cells
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| Error::Invalid("cell".into()))?;
    let mut g = p.graph.clone();
    let root = dependencies::resolve(&mut g, p.current(), c.root)?;
    let expression = encode(&g, root)?;
    // Never export a context that the bounded notebook source editor cannot import.
    let source = latex::source_from_graph(&g, root)?;
    if source.len() > 2048 {
        return Err(Error::Limit);
    }
    latex::preview(&source)?;
    Ok(Exchange {
        format: "symbio.mathjson".into(),
        version: 1,
        name: c.name.clone(),
        expression,
        assumptions: p.current().assumptions.clone(),
    })
}
pub fn to_bytes(exchange: &Exchange) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec_pretty(exchange)?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
pub fn import(bytes: &[u8]) -> Result<AppCore> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let value: Value = serde_json::from_slice(bytes)?;
    let exchange = if value.as_object().is_some_and(|o| o.contains_key("format")) {
        let e: Exchange = serde_json::from_value(value)?;
        if e.format != "symbio.mathjson" || e.version != 1 {
            return Err(Error::Version);
        }
        e
    } else {
        Exchange {
            format: "symbio.mathjson".into(),
            version: 1,
            name: "math".into(),
            expression: value,
            assumptions: BTreeMap::new(),
        }
    };
    if !graph::valid_symbol(&exchange.name) || exchange.assumptions.len() > 64 {
        return Err(Error::Invalid("MathJSON context".into()));
    }
    let mut g = Graph::default();
    let root = decode(&mut g, &exchange.expression)?;
    g.rebuild()?;
    structured::shape(&g, root)?;
    let source = latex::source_from_graph(&g, root)?;
    let mut project = Project::empty("MathJSON")?;
    project.current_mut().assumptions = exchange.assumptions;
    let mut core = AppCore::new(project)?;
    core.apply(Request {
        expected_revision: 0,
        actor: Actor::Human,
        command: Command::DefineLatex {
            id: None,
            name: exchange.name,
            source,
        },
    })?;
    Ok(core)
}
pub fn read(path: impl AsRef<std::path::Path>) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
pub fn write_new(path: impl AsRef<std::path::Path>, exchange: &Exchange) -> Result<()> {
    use std::io::Write;
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let mut f = tempfile::NamedTempFile::new_in(parent)?;
    f.write_all(&to_bytes(exchange)?)?;
    f.as_file().sync_all()?;
    f.persist_noclobber(path).map_err(|e| Error::Io(e.error))?;
    Ok(())
}
