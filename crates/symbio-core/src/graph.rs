//! Immutable, structurally interned DAG. Node IDs are append-only and document-local.
use crate::{Error, Result};
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
pub type Id = usize;
pub type Rational = BigRational;
pub const MAX_NODES: usize = 16000;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    tag = "op",
    content = "args",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Node {
    Number(String),
    Symbol(String),
    Add(Id, Id),
    Sub(Id, Id),
    Mul(Id, Id),
    Div(Id, Id),
    Pow(Id, Id),
    Call(String, Id),
}
impl Node {
    pub fn children(&self) -> Vec<Id> {
        match self {
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b)
            | Self::Pow(a, b) => vec![*a, *b],
            Self::Call(_, a) => vec![*a],
            _ => vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub nodes: Vec<Node>,
    #[serde(skip)]
    index: HashMap<Node, Id>,
}
pub fn rational(s: &str) -> Result<Rational> {
    if s.len() > 4096 {
        return Err(Error::Limit);
    }
    let (a, b) = s.split_once('/').unwrap_or((s, "1"));
    let n = a
        .parse::<BigInt>()
        .map_err(|_| Error::Invalid("number".into()))?;
    let d = b
        .parse::<BigInt>()
        .map_err(|_| Error::Invalid("number".into()))?;
    if d.is_zero() {
        return Err(Error::Domain("division by zero".into()));
    }
    Ok(Rational::new(n, d))
}
pub fn rat_text(r: &Rational) -> String {
    if r.denom().is_one() {
        r.numer().to_string()
    } else {
        format!("{}/{}", r.numer(), r.denom())
    }
}
pub fn valid_symbol(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 48
        && s.chars()
            .enumerate()
            .all(|(i, c)| c.is_ascii_alphabetic() || c == '_' || (i > 0 && c.is_ascii_digit()))
}
impl Graph {
    pub fn rebuild(&mut self) -> Result<()> {
        if self.nodes.len() > MAX_NODES {
            return Err(Error::Limit);
        }
        self.index.clear();
        let mut depths = vec![];
        for (i, n) in self.nodes.iter().enumerate() {
            if n.children().iter().any(|c| *c >= i) {
                return Err(Error::Invalid("DAG reference".into()));
            }
            let d = 1 + n.children().iter().map(|c| depths[*c]).max().unwrap_or(0);
            if d > 96 {
                return Err(Error::Limit);
            }
            depths.push(d);
            match n {
                Node::Number(s) => {
                    let r = rational(s)?;
                    if rat_text(&r) != *s {
                        return Err(Error::Invalid("canonical number".into()));
                    }
                }
                Node::Symbol(s) => {
                    if !valid_symbol(s) {
                        return Err(Error::Invalid("symbol".into()));
                    }
                }
                Node::Call(f, _)
                    if !["sin", "cos", "exp", "log", "sqrt", "abs"].contains(&f.as_str()) =>
                {
                    return Err(Error::Unsupported(f.clone()));
                }
                _ => {}
            }
            if self.index.insert(n.clone(), i).is_some() {
                return Err(Error::Invalid("duplicate DAG node".into()));
            }
        }
        Ok(())
    }
    pub fn intern(&mut self, n: Node) -> Result<Id> {
        if let Some(id) = self.index.get(&n) {
            return Ok(*id);
        }
        if self.nodes.len() >= MAX_NODES {
            return Err(Error::Limit);
        }
        let id = self.nodes.len();
        self.nodes.push(n.clone());
        self.index.insert(n, id);
        Ok(id)
    }
    pub fn number(&mut self, n: i64) -> Result<Id> {
        self.intern(Node::Number(n.to_string()))
    }
    pub fn rat(&mut self, r: Rational) -> Result<Id> {
        let s = rat_text(&r);
        if s.len() > 4096 {
            return Err(Error::Limit);
        }
        self.intern(Node::Number(s))
    }
    pub fn value(&self, id: Id) -> Option<Rational> {
        match self.nodes.get(id)? {
            Node::Number(s) => rational(s).ok(),
            _ => None,
        }
    }
    pub fn symbols(&self, id: Id) -> BTreeSet<String> {
        let mut symbols = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let mut pending = vec![id];
        while let Some(i) = pending.pop() {
            if !seen.insert(i) {
                continue;
            }
            if let Some(n) = self.nodes.get(i) {
                if let Node::Symbol(s) = n {
                    symbols.insert(s.clone());
                }
                pending.extend(n.children());
            }
        }
        symbols
    }
    pub fn text(&self, id: Id) -> String {
        self.format(id, false, 0).unwrap_or_else(|_| "…".into())
    }
    pub fn mathml(&self, id: Id) -> String {
        format!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\">{}</math>",
            self.pretty_math(id, 0, 0)
                .unwrap_or_else(|_| "<mtext>…</mtext>".into())
        )
    }
    fn pretty_math(&self, id: Id, parent: u8, depth: usize) -> Result<String> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        let node = &self.nodes[id];
        let precedence = match node {
            Node::Add(..) | Node::Sub(..) => 1,
            Node::Mul(..) | Node::Div(..) => 2,
            Node::Pow(..) => 3,
            _ => 4,
        };
        let f = |i, p| self.pretty_math(i, p, depth + 1);
        let text = match node {
            Node::Number(_) | Node::Symbol(_) => self.format(id, true, depth)?,
            Node::Add(a, b) => format!("<mrow>{}<mo>+</mo>{}</mrow>", f(*a, 1)?, f(*b, 1)?),
            Node::Sub(a, b) => format!("<mrow>{}<mo>−</mo>{}</mrow>", f(*a, 1)?, f(*b, 2)?),
            Node::Mul(a, b) => format!("<mrow>{}<mo>·</mo>{}</mrow>", f(*a, 2)?, f(*b, 2)?),
            Node::Div(a, b) => format!("<mfrac>{}{}</mfrac>", f(*a, 0)?, f(*b, 0)?),
            Node::Pow(a, b) => {
                let mut base = f(*a, 4)?;
                if self.value(*a).is_some_and(|n| n < Rational::zero()) {
                    base = format!("<mrow><mo>(</mo>{base}<mo>)</mo></mrow>")
                }
                format!("<msup>{base}{}</msup>", f(*b, 0)?)
            }
            Node::Call(name, a) => {
                let a = f(*a, 0)?;
                match name.as_str() {
                    "sqrt" => format!("<msqrt>{a}</msqrt>"),
                    "abs" => format!("<mrow><mo>|</mo>{a}<mo>|</mo></mrow>"),
                    _ => format!("<mrow><mi>{name}</mi><mo>(</mo>{a}<mo>)</mo></mrow>"),
                }
            }
        };
        if text.len() > 32768 {
            return Err(Error::Limit);
        }
        Ok(if precedence < parent && !matches!(node, Node::Div(..)) {
            format!("<mrow><mo>(</mo>{text}<mo>)</mo></mrow>")
        } else {
            text
        })
    }
    fn format(&self, id: Id, math: bool, depth: usize) -> Result<String> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        let f = |i| self.format(i, math, depth + 1);
        let result = match &self.nodes[id] {
            Node::Number(n) => {
                if math {
                    if let Some((a, b)) = n.split_once('/') {
                        format!("<mfrac><mn>{a}</mn><mn>{b}</mn></mfrac>")
                    } else {
                        format!("<mn>{n}</mn>")
                    }
                } else {
                    n.clone()
                }
            }
            Node::Symbol(s) => {
                if math {
                    format!("<mi>{s}</mi>")
                } else {
                    s.clone()
                }
            }
            Node::Call(name, a) => {
                let a = f(*a)?;
                if math {
                    match name.as_str() {
                        "sqrt" => format!("<msqrt>{a}</msqrt>"),
                        "abs" => format!("<mrow><mo>|</mo>{a}<mo>|</mo></mrow>"),
                        _ => format!("<mrow><mi>{name}</mi><mo>(</mo>{a}<mo>)</mo></mrow>"),
                    }
                } else {
                    format!("{name}({a})")
                }
            }
            n => {
                let cs = n.children();
                let a = f(cs[0])?;
                let b = f(cs[1])?;
                let op = match n {
                    Node::Add(..) => "+",
                    Node::Sub(..) => "−",
                    Node::Mul(..) => "·",
                    Node::Div(..) => "/",
                    _ => "^",
                };
                if math {
                    match n {
                        Node::Div(..) => format!("<mfrac>{a}{b}</mfrac>"),
                        Node::Pow(..) => {
                            format!("<msup><mrow><mo>(</mo>{a}<mo>)</mo></mrow>{b}</msup>")
                        }
                        _ => format!("<mrow><mo>(</mo>{a}<mo>{op}</mo>{b}<mo>)</mo></mrow>"),
                    }
                } else {
                    format!(
                        "({a} {} {b})",
                        match n {
                            Node::Sub(..) => "-",
                            Node::Mul(..) => "*",
                            _ => op,
                        }
                    )
                }
            }
        };
        if result.len() > 32768 {
            return Err(Error::Limit);
        }
        Ok(result)
    }
    pub fn numeric(&self, id: Id, vars: &BTreeMap<String, f64>) -> Result<f64> {
        let mut values: HashMap<Id, f64> = HashMap::new();
        let mut reachable = BTreeSet::new();
        let mut pending = vec![id];
        while let Some(i) = pending.pop() {
            if reachable.insert(i) {
                pending.extend(self.nodes[i].children());
            }
        }
        for i in reachable {
            let v = match &self.nodes[i] {
                Node::Number(s) => rational(s)?.to_f64().ok_or(Error::Limit)?,
                Node::Symbol(s) => *vars
                    .get(s)
                    .ok_or_else(|| Error::Domain(format!("missing {s}")))?,
                Node::Add(a, b) => values[a] + values[b],
                Node::Sub(a, b) => values[a] - values[b],
                Node::Mul(a, b) => values[a] * values[b],
                Node::Div(a, b) => values[a] / values[b],
                Node::Pow(a, b) => {
                    if values[a] == 0. && values[b] <= 0. {
                        return Err(Error::Domain("undefined power".into()));
                    }
                    values[a].powf(values[b])
                }
                Node::Call(f, a) => match f.as_str() {
                    "sin" => values[a].sin(),
                    "cos" => values[a].cos(),
                    "exp" => values[a].exp(),
                    "log" => values[a].ln(),
                    "sqrt" => values[a].sqrt(),
                    "abs" => values[a].abs(),
                    _ => return Err(Error::Unsupported(f.clone())),
                },
            };
            if !v.is_finite() {
                return Err(Error::Domain("non-finite result".into()));
            }
            values.insert(i, v);
        }
        values
            .get(&id)
            .copied()
            .ok_or_else(|| Error::Invalid("root".into()))
    }
}
