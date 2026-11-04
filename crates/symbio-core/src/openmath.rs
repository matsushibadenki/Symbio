//! Strict exact OpenMath XML phrasebook. Unknown semantics are never discarded.
use crate::{
    mathjson::{self, Exchange},
    *,
};
use quick_xml::{
    NsReader,
    events::{BytesStart, Event},
    name::ResolveResult,
};
use serde_json::{Value, json};
const NS: &str = "http://www.openmath.org/OpenMath";
const CONTEXT_BASE: &str = "urn:symbio:cd";
pub const MAX_BYTES: usize = mathjson::MAX_BYTES;
fn unsupported() -> Error {
    Error::Unsupported("OpenMath exact subset".into())
}
#[derive(Default, Debug)]
struct Element {
    name: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<Element>,
}
fn start(e: &BytesStart<'_>) -> Result<Element> {
    let mut attrs = BTreeMap::new();
    for a in e.attributes() {
        let a = a.map_err(|_| unsupported())?;
        let value = a
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|_| unsupported())?
            .into_owned();
        if a.key.0 == "xmlns" || a.key.0.starts_with("xmlns:") {
            if value != NS {
                return Err(unsupported());
            }
            continue;
        }
        if attrs.insert(a.key.0.into(), value).is_some() {
            return Err(unsupported());
        }
    }
    Ok(Element {
        name: e.name().local_name().as_ref().into(),
        attrs,
        ..Default::default()
    })
}
fn attach(stack: &mut [Element], root: &mut Option<Element>, e: Element) -> Result<()> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(e);
    } else if root.replace(e).is_some() {
        return Err(unsupported());
    }
    Ok(())
}
fn parse(bytes: &[u8]) -> Result<Element> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| unsupported())?;
    if text.chars().any(|c| {
        !matches!(c, '\t' | '\n' | '\r')
            && ((c as u32) < 0x20 || matches!(c, '\u{fffe}' | '\u{ffff}'))
    }) {
        return Err(unsupported());
    }
    let mut reader = NsReader::from_str(text);
    let mut stack = Vec::new();
    let mut root = None;
    let mut count = 0;
    let mut declared = false;
    loop {
        let (namespace, event) = reader.read_resolved_event().map_err(|_| unsupported())?;
        let known_namespace = matches!(namespace,ResolveResult::Bound(ns) if ns.0==NS);
        if matches!(event, Event::Start(_) | Event::Empty(_) | Event::End(_)) && !known_namespace {
            return Err(unsupported());
        }
        match event {
            Event::Start(e) => {
                count += 1;
                if count > 4096 || stack.len() > 48 {
                    return Err(Error::Limit);
                }
                stack.push(start(&e)?);
            }
            Event::Empty(e) => {
                count += 1;
                if count > 4096 || stack.len() > 48 {
                    return Err(Error::Limit);
                }
                attach(&mut stack, &mut root, start(&e)?)?;
            }
            Event::End(e) => {
                let node = stack.pop().ok_or_else(unsupported)?;
                if node.name != e.name().local_name().as_ref() {
                    return Err(unsupported());
                }
                attach(&mut stack, &mut root, node)?;
            }
            Event::Text(e) => {
                let value = e.xml10_content();
                if let Some(parent) = stack.last_mut() {
                    parent.text.push_str(&value);
                } else if !value.trim().is_empty() {
                    return Err(unsupported());
                }
            }
            Event::GeneralRef(e) => {
                let escaped = format!("&{};", e.as_ref());
                let value = quick_xml::escape::unescape(&escaped).map_err(|_| unsupported())?;
                stack
                    .last_mut()
                    .ok_or_else(unsupported)?
                    .text
                    .push_str(&value);
            }
            Event::Decl(e) => {
                if declared
                    || !stack.is_empty()
                    || root.is_some()
                    || e.version().map_err(|_| unsupported())?.as_ref() != "1.0"
                    || e.encoding()
                        .transpose()
                        .map_err(|_| unsupported())?
                        .is_some_and(|v| !v.eq_ignore_ascii_case("utf-8"))
                {
                    return Err(unsupported());
                }
                declared = true;
            }
            Event::Comment(_) => {}
            Event::Eof => break,
            // No DTD, CDATA, processing instructions, external entities or foreign objects.
            _ => return Err(unsupported()),
        }
    }
    if !stack.is_empty() {
        return Err(unsupported());
    }
    root.ok_or_else(unsupported)
}
impl Element {
    fn check(&self, name: &str, attrs: &[&str], text: bool) -> Result<()> {
        if self.name != name
            || self.attrs.keys().any(|k| !attrs.contains(&k.as_str()))
            || (!text && !self.text.trim().is_empty())
        {
            return Err(unsupported());
        }
        Ok(())
    }
    fn leaf(&self, name: &str, attrs: &[&str], text: bool) -> Result<()> {
        self.check(name, attrs, text)?;
        if !self.children.is_empty() {
            return Err(unsupported());
        }
        Ok(())
    }
    fn symbol(&self) -> Result<(&str, &str)> {
        self.leaf("OMS", &["cd", "name", "cdbase"], false)?;
        if self
            .attrs
            .get("cdbase")
            .is_some_and(|b| b != "http://www.openmath.org/cd" && b != CONTEXT_BASE)
        {
            return Err(unsupported());
        }
        Ok((
            self.attrs.get("cd").ok_or_else(unsupported)?,
            self.attrs.get("name").ok_or_else(unsupported)?,
        ))
    }
}
// Operators consuming functions must keep the lambda and its explicit scope.
// Standalone functions and arbitrary application/substitution are not scalar SEG objects.
fn application<'a>(e: &'a Element, cd: &str, name: &str, arity: usize) -> Result<&'a [Element]> {
    e.check("OMA", &[], false)?;
    if e.children.len() != arity + 1 {
        return Err(unsupported());
    }
    official(&e.children[0], cd, name)?;
    Ok(&e.children[1..])
}
fn official(e: &Element, cd: &str, name: &str) -> Result<()> {
    if e.symbol()? != (cd, name) || e.attrs.get("cdbase").is_some_and(|b| b == CONTEXT_BASE) {
        return Err(unsupported());
    }
    Ok(())
}
fn lambda(e: &Element) -> Result<(&str, &Element)> {
    e.check("OMBIND", &[], false)?;
    if e.children.len() != 3 {
        return Err(unsupported());
    }
    official(&e.children[0], "fns1", "lambda")?;
    let variables = &e.children[1];
    variables.check("OMBVAR", &[], false)?;
    if variables.children.len() != 1 {
        return Err(unsupported());
    }
    let variable = &variables.children[0];
    variable.leaf("OMV", &["name"], false)?;
    Ok((
        variable.attrs.get("name").ok_or_else(unsupported)?,
        &e.children[2],
    ))
}
const MAPPINGS: &[(&str, &str, &str)] = &[
    ("arith1", "plus", "Add"),
    ("arith1", "minus", "Subtract"),
    ("arith1", "times", "Multiply"),
    ("arith1", "divide", "Divide"),
    ("arith1", "power", "Power"),
    ("arith1", "unary_minus", "Negate"),
    ("arith1", "abs", "Abs"),
    ("nums1", "rational", "Rational"),
    ("relation1", "eq", "Equal"),
    ("transc1", "sin", "Sin"),
    ("transc1", "cos", "Cos"),
    ("transc1", "exp", "Exp"),
    ("transc1", "ln", "Ln"),
    ("rounding1", "floor", "Floor"),
    ("rounding1", "ceiling", "Ceil"),
    ("linalg1", "determinant", "Determinant"),
    ("linalg1", "transpose", "Transpose"),
];
fn decode(e: &Element) -> Result<Value> {
    match e.name.as_str() {
        "OMI" => {
            e.leaf("OMI", &[], true)?;
            let s = e.text.trim();
            let digits = s
                .strip_prefix('-')
                .or_else(|| s.strip_prefix('+'))
                .unwrap_or(s);
            if digits.is_empty()
                || digits.len() > 4096
                || !digits.bytes().all(|c| c.is_ascii_digit())
            {
                return Err(unsupported());
            }
            Ok(json!({"num":s.strip_prefix('+').unwrap_or(s)}))
        }
        "OMV" => {
            e.leaf("OMV", &["name"], false)?;
            Ok(json!({"sym":e.attrs.get("name").ok_or_else(unsupported)?}))
        }
        "OMS" => {
            let (cd, name) = e.symbol()?;
            if e.attrs.get("cdbase").is_some_and(|b| b == CONTEXT_BASE) {
                return Err(unsupported());
            }
            match (cd, name) {
                ("nums1", "pi") => Ok(json!("Pi")),
                ("nums1", "e") => Ok(json!("ExponentialE")),
                _ => Err(unsupported()),
            }
        }
        "OMA" => {
            e.check("OMA", &[], false)?;
            let first = e.children.first().ok_or_else(unsupported)?;
            // diff/int return functions. Only their application at the SAME named
            // variable maps directly to our expression derivative/primitive. A different
            // point needs capture-avoiding substitution and is deliberately rejected.
            if first.name == "OMA" {
                first.check("OMA", &[], false)?;
                if e.children.len() != 2 || first.children.len() != 2 {
                    return Err(unsupported());
                }
                let operator = &first.children[0];
                let head = match operator.symbol()? {
                    ("calculus1", "diff") => "D",
                    ("calculus1", "int") => "Integrate",
                    _ => return Err(unsupported()),
                };
                official(
                    operator,
                    "calculus1",
                    if head == "D" { "diff" } else { "int" },
                )?;
                let (variable, body) = lambda(&first.children[1])?;
                let point = &e.children[1];
                point.leaf("OMV", &["name"], false)?;
                if point.attrs.get("name").map(String::as_str) != Some(variable) {
                    return Err(unsupported());
                }
                return Ok(json!([head, decode(body)?, variable]));
            }
            let (cd, name) = first.symbol()?;
            if first.attrs.get("cdbase").is_some_and(|b| b == CONTEXT_BASE) {
                return Err(unsupported());
            }
            if matches!(
                (cd, name),
                ("calculus1", "defint") | ("arith1", "sum" | "product")
            ) {
                let args = application(e, cd, name, 2)?;
                let interval = application(
                    &args[0],
                    "interval1",
                    if cd == "calculus1" {
                        "oriented_interval"
                    } else {
                        "integer_interval"
                    },
                    2,
                )?;
                let (variable, body) = lambda(&args[1])?;
                return Ok(json!([
                    match name {
                        "defint" => "Integrate",
                        "sum" => "Sum",
                        _ => "Product",
                    },
                    decode(body)?,
                    [
                        "Tuple",
                        variable,
                        decode(&interval[0])?,
                        decode(&interval[1])?
                    ]
                ]));
            }
            if (cd, name) == ("linalg2", "matrix") {
                let mut rows = vec![json!("List")];
                for row in &e.children[1..] {
                    row.check("OMA", &[], false)?;
                    let h = row.children.first().ok_or_else(unsupported)?;
                    if h.symbol()? != ("linalg2", "matrixrow")
                        || h.attrs.get("cdbase").is_some_and(|b| b == CONTEXT_BASE)
                    {
                        return Err(unsupported());
                    }
                    let mut entries = vec![json!("List")];
                    for entry in &row.children[1..] {
                        entries.push(decode(entry)?);
                    }
                    rows.push(Value::Array(entries));
                }
                return Ok(Value::Array(rows));
            }
            let head = if (cd, name) == ("arith1", "root") {
                if e.children.len() != 3 || decode(&e.children[2])? != json!({"num":"2"}) {
                    return Err(unsupported());
                }
                "Sqrt"
            } else {
                MAPPINGS
                    .iter()
                    .find(|m| m.0 == cd && m.1 == name)
                    .map(|m| m.2)
                    .ok_or_else(unsupported)?
            };
            let mut args = vec![json!(head)];
            for child in &e.children[1..if head == "Sqrt" { 2 } else { e.children.len() }] {
                args.push(decode(child)?);
            }
            Ok(Value::Array(args))
        }
        _ => Err(unsupported()),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Context {
    version: u32,
    name: String,
    assumptions: BTreeMap<String, Assumption>,
}
fn check_scope(expression: &Value) -> Result<()> {
    let mut graph = graph::Graph::default();
    let root = mathjson::decode(&mut graph, expression)?;
    structured::shape(&graph, root)?;
    for (id, node) in graph.nodes.iter().enumerate() {
        if !matches!(node, graph::Node::Matrix { .. } | graph::Node::Equation(..))
            && !matches!(node, graph::Node::Call(f, _) if f == "transpose")
            && structured::shape(&graph, id)? != structured::Shape::Scalar
        {
            return Err(unsupported());
        }
    }
    Ok(())
}
pub fn import(bytes: &[u8]) -> Result<AppCore> {
    let root = parse(bytes)?;
    root.check("OMOBJ", &["version", "cdbase"], false)?;
    if root
        .attrs
        .get("cdbase")
        .is_some_and(|v| v != "http://www.openmath.org/cd")
        || root.attrs.get("version").is_some_and(|v| v != "2.0")
        || root.children.len() != 1
    {
        return Err(unsupported());
    }
    let mut expression = &root.children[0];
    let mut context = Context {
        version: 1,
        name: "math".into(),
        assumptions: BTreeMap::new(),
    };
    if expression.name == "OMATTR" {
        expression.check("OMATTR", &[], false)?;
        if expression.children.len() != 2 {
            return Err(unsupported());
        }
        let attributes = &expression.children[0];
        attributes.check("OMATP", &[], false)?;
        if attributes.children.len() != 2 {
            return Err(unsupported());
        }
        let key = &attributes.children[0];
        if key.symbol()? != ("symbio_context1", "context")
            || key.attrs.get("cdbase").map(String::as_str) != Some(CONTEXT_BASE)
        {
            return Err(unsupported());
        }
        let value = &attributes.children[1];
        value.leaf("OMSTR", &[], true)?;
        context = serde_json::from_str(&value.text)?;
        if context.version != 1 {
            return Err(Error::Version);
        }
        expression = &expression.children[1];
    }
    let exchange = Exchange {
        format: "symbio.mathjson".into(),
        version: 1,
        name: context.name,
        expression: decode(expression)?,
        assumptions: context.assumptions,
    };
    check_scope(&exchange.expression)?;
    mathjson::import_as(&mathjson::to_bytes(&exchange)?, "OpenMath")
}
fn escape(s: &str) -> String {
    quick_xml::escape::escape(s).into_owned()
}
fn symbol(cd: &str, name: &str) -> String {
    format!("<OMS cd=\"{cd}\" name=\"{name}\"/>")
}
fn encode_lambda(variable: &str, body: &str) -> String {
    format!(
        "<OMBIND>{}<OMBVAR><OMV name=\"{}\"/></OMBVAR>{body}</OMBIND>",
        symbol("fns1", "lambda"),
        escape(variable)
    )
}
fn encode(v: &Value) -> Result<String> {
    if let Some(n) = v.get("num").and_then(Value::as_str) {
        return Ok(format!("<OMI>{}</OMI>", escape(n)));
    }
    if let Some(s) = v.as_str() {
        return Ok(match s {
            "Pi" => symbol("nums1", "pi"),
            "ExponentialE" => symbol("nums1", "e"),
            _ => format!("<OMV name=\"{}\"/>", escape(s)),
        });
    }
    let a = v.as_array().ok_or_else(unsupported)?;
    let head = a.first().and_then(Value::as_str).ok_or_else(unsupported)?;
    if matches!(head, "D" | "Integrate" | "Sum" | "Product") {
        let body = encode(&a[1])?;
        if let Some(variable) = a[2].as_str() {
            let function = encode_lambda(variable, &body);
            return Ok(format!(
                "<OMA><OMA>{}{function}</OMA><OMV name=\"{}\"/></OMA>",
                symbol("calculus1", if head == "D" { "diff" } else { "int" }),
                escape(variable)
            ));
        }
        // MathJSON's validated Tuple puts limits outside the bound body's scope.
        let limits = a[2].as_array().ok_or_else(unsupported)?;
        let variable = limits[1].as_str().ok_or_else(unsupported)?;
        let function = encode_lambda(variable, &body);
        return Ok(format!(
            "<OMA>{}<OMA>{}{}{}</OMA>{function}</OMA>",
            match head {
                "Integrate" => symbol("calculus1", "defint"),
                "Sum" => symbol("arith1", "sum"),
                _ => symbol("arith1", "product"),
            },
            symbol(
                "interval1",
                if head == "Integrate" {
                    "oriented_interval"
                } else {
                    "integer_interval"
                }
            ),
            encode(&limits[2])?,
            encode(&limits[3])?
        ));
    }
    if head == "List" {
        let mut result = format!("<OMA>{}", symbol("linalg2", "matrix"));
        for row in &a[1..] {
            let row = row
                .as_array()
                .filter(|a| a.first() == Some(&json!("List")))
                .ok_or_else(unsupported)?;
            result.push_str(&format!("<OMA>{}", symbol("linalg2", "matrixrow")));
            for value in &row[1..] {
                result.push_str(&encode(value)?);
            }
            result.push_str("</OMA>");
        }
        result.push_str("</OMA>");
        return Ok(result);
    }
    let h = if head == "Sqrt" {
        symbol("arith1", "root")
    } else {
        let m = MAPPINGS
            .iter()
            .find(|m| m.2 == head)
            .ok_or_else(unsupported)?;
        symbol(m.0, m.1)
    };
    let mut result = format!("<OMA>{h}");
    for value in &a[1..] {
        result.push_str(&encode(value)?);
    }
    if head == "Sqrt" {
        result.push_str("<OMI>2</OMI>");
    }
    result.push_str("</OMA>");
    Ok(result)
}
pub fn export(p: &Project, id: u64) -> Result<Vec<u8>> {
    let e = mathjson::export(p, id)?;
    check_scope(&e.expression)?;
    let expression = encode(&e.expression)?;
    let context = escape(&serde_json::to_string(&Context {
        version: 1,
        name: e.name,
        assumptions: e.assumptions,
    })?);
    let result=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<OMOBJ xmlns=\"{NS}\" version=\"2.0\"><OMATTR><OMATP><OMS cdbase=\"{CONTEXT_BASE}\" cd=\"symbio_context1\" name=\"context\"/><OMSTR>{context}</OMSTR></OMATP>{expression}</OMATTR></OMOBJ>\n").into_bytes();
    if result.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(result)
}
pub fn write_new(path: impl AsRef<std::path::Path>, bytes: &[u8]) -> Result<()> {
    mathjson::write_bytes_new(path, bytes)
}
