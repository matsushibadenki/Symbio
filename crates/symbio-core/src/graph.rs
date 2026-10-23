//! Immutable, structurally interned DAG. Node IDs are append-only between collections and revision-local.
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
    Reference(String),
    Add(Id, Id),
    Sub(Id, Id),
    Mul(Id, Id),
    Div(Id, Id),
    Pow(Id, Id),
    Call(String, Id),
    Constant(String),
    Integral {
        body: Id,
        variable: String,
        lower: Option<Id>,
        upper: Option<Id>,
    },
    Derivative {
        body: Id,
        variable: String,
    },
    Aggregate {
        body: Id,
        variable: String,
        lower: Id,
        upper: Id,
        product: bool,
    },
    Matrix {
        rows: usize,
        cols: usize,
        entries: Vec<Id>,
    },
    Equation(Id, Id),
    Solutions {
        variable: String,
        values: Vec<Id>,
    },
}
impl Node {
    pub fn children(&self) -> Vec<Id> {
        match self {
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b)
            | Self::Pow(a, b)
            | Self::Equation(a, b) => vec![*a, *b],
            Self::Call(_, a) | Self::Derivative { body: a, .. } => vec![*a],
            Self::Integral {
                body, lower, upper, ..
            } => std::iter::once(*body)
                .chain(lower.iter().copied())
                .chain(upper.iter().copied())
                .collect(),
            Self::Aggregate {
                body, lower, upper, ..
            } => vec![*body, *lower, *upper],
            Self::Matrix { entries, .. } => entries.clone(),
            Self::Solutions { values, .. } => values.clone(),
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
                Node::Symbol(s) | Node::Reference(s) => {
                    if !valid_symbol(s) {
                        return Err(Error::Invalid("symbol".into()));
                    }
                }
                Node::Call(f, _)
                    if ![
                        "sin",
                        "cos",
                        "exp",
                        "log",
                        "sqrt",
                        "abs",
                        "det",
                        "transpose",
                        "inverse",
                        "floor",
                        "ceil",
                    ]
                    .contains(&f.as_str()) =>
                {
                    return Err(Error::Unsupported(f.clone()));
                }
                Node::Constant(s) if !["pi", "e"].contains(&s.as_str()) => {
                    return Err(Error::Invalid("constant".into()));
                }
                Node::Integral {
                    variable,
                    lower,
                    upper,
                    ..
                } => {
                    if !valid_symbol(variable) || lower.is_some() != upper.is_some() {
                        return Err(Error::Invalid("integral".into()));
                    }
                }
                Node::Derivative { variable, .. }
                | Node::Aggregate { variable, .. }
                | Node::Solutions { variable, .. }
                    if !valid_symbol(variable) =>
                {
                    return Err(Error::Invalid("bound variable".into()));
                }
                Node::Matrix {
                    rows,
                    cols,
                    entries,
                } if *rows == 0
                    || *cols == 0
                    || *rows > 8
                    || *cols > 8
                    || rows.checked_mul(*cols) != Some(entries.len()) =>
                {
                    return Err(Error::Invalid("matrix shape".into()));
                }
                Node::Solutions { values, .. } if values.len() > 2 => return Err(Error::Limit),
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
    /// Free symbols respect integral/sum binders. An indefinite integral's variable
    /// remains free in its antiderivative; a definite integral's variable does not.
    pub fn symbols(&self, id: Id) -> BTreeSet<String> {
        fn go(g: &Graph, id: Id, memo: &mut HashMap<Id, BTreeSet<String>>) -> BTreeSet<String> {
            if let Some(s) = memo.get(&id) {
                return s.clone();
            }
            let n = &g.nodes[id];
            let mut s = BTreeSet::new();
            match n {
                Node::Symbol(name) => {
                    s.insert(name.clone());
                }
                Node::Integral {
                    body,
                    variable,
                    lower: Some(l),
                    upper: Some(u),
                }
                | Node::Aggregate {
                    body,
                    variable,
                    lower: l,
                    upper: u,
                    ..
                } => {
                    s = go(g, *body, memo);
                    s.remove(variable);
                    s.extend(go(g, *l, memo));
                    s.extend(go(g, *u, memo));
                }
                Node::Integral {
                    body,
                    variable,
                    lower: None,
                    ..
                } => {
                    s = go(g, *body, memo);
                    s.insert(variable.clone());
                }
                _ => {
                    for child in n.children() {
                        s.extend(go(g, child, memo));
                    }
                }
            }
            memo.insert(id, s.clone());
            s
        }
        go(self, id, &mut HashMap::new())
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
            Node::Equation(..) => 0,
            Node::Add(..) | Node::Sub(..) => 1,
            Node::Mul(..) | Node::Div(..) => 2,
            Node::Pow(..) => 3,
            _ => 4,
        };
        let f = |i, p| self.pretty_math(i, p, depth + 1);
        let text = match node {
            Node::Integral { .. }
            | Node::Derivative { .. }
            | Node::Aggregate { .. }
            | Node::Matrix { .. }
            | Node::Equation(..)
            | Node::Solutions { .. } => self.structured_math(id, depth)?,
            Node::Number(_) | Node::Symbol(_) | Node::Reference(_) | Node::Constant(_) => {
                self.format(id, true, depth)?
            }
            Node::Add(a, b) => format!("<mrow>{}<mo>+</mo>{}</mrow>", f(*a, 1)?, f(*b, 1)?),
            Node::Sub(a, b) => format!("<mrow>{}<mo>−</mo>{}</mrow>", f(*a, 1)?, f(*b, 2)?),
            Node::Mul(a, b) => {
                let op = if self.value(*a).is_some()
                    && matches!(self.nodes[*b], Node::Symbol(_) | Node::Pow(..))
                {
                    "<mo>&#x2062;</mo>"
                } else {
                    "<mo>·</mo>"
                };
                if self.value(*a) == Some(Rational::from_integer((-1).into())) {
                    format!("<mrow><mo>−</mo>{}</mrow>", f(*b, 2)?)
                } else {
                    format!("<mrow>{}{op}{}</mrow>", f(*a, 2)?, f(*b, 2)?)
                }
            }
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
                    "transpose" => format!("<msup>{a}<mi mathvariant=\"normal\">T</mi></msup>"),
                    "inverse" => format!("<msup>{a}<mrow><mo>−</mo><mn>1</mn></mrow></msup>"),
                    "floor" => format!("<mrow><mo>⌊</mo>{a}<mo>⌋</mo></mrow>"),
                    "ceil" => format!("<mrow><mo>⌈</mo>{a}<mo>⌉</mo></mrow>"),
                    _ => format!(
                        "<mrow><mi mathvariant=\"normal\">{name}</mi><mo>&#x2061;</mo><mo>(</mo>{a}<mo>)</mo></mrow>"
                    ),
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
            Node::Reference(s) => {
                if math {
                    format!("<mrow><mo>@</mo><mi mathvariant=\"normal\">{s}</mi></mrow>")
                } else {
                    format!("@{s}")
                }
            }
            Node::Constant(s) => {
                if math {
                    format!(
                        "<mi mathvariant=\"normal\">{}</mi>",
                        if s == "pi" { "π" } else { "e" }
                    )
                } else {
                    s.clone()
                }
            }
            Node::Integral {
                body,
                variable,
                lower,
                upper,
            } => format!(
                "integral({}, {variable}, {}, {})",
                f(*body)?,
                lower.map(f).transpose()?.unwrap_or_default(),
                upper.map(f).transpose()?.unwrap_or_default()
            ),
            Node::Derivative { body, variable } => {
                format!("differentiate({}, {variable})", f(*body)?)
            }
            Node::Aggregate {
                body,
                variable,
                lower,
                upper,
                product,
            } => format!(
                "{}({}, {variable}, {}, {})",
                if *product { "product" } else { "sum" },
                f(*body)?,
                f(*lower)?,
                f(*upper)?
            ),
            Node::Matrix {
                rows,
                cols,
                entries,
            } => format!(
                "matrix({rows}x{cols}; {})",
                entries
                    .iter()
                    .map(|n| f(*n))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ),
            Node::Equation(a, b) => format!("{} = {}", f(*a)?, f(*b)?),
            Node::Solutions { variable, values } => format!(
                "{variable} in {{{}}}",
                values
                    .iter()
                    .map(|n| f(*n))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ),

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
                    symbol_math(s)
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
                        "transpose" => format!("<msup>{a}<mi mathvariant=\"normal\">T</mi></msup>"),
                        "inverse" => format!("<msup>{a}<mrow><mo>−</mo><mn>1</mn></mrow></msup>"),
                        "floor" => format!("<mrow><mo>⌊</mo>{a}<mo>⌋</mo></mrow>"),
                        "ceil" => format!("<mrow><mo>⌈</mo>{a}<mo>⌉</mo></mrow>"),
                        _ => format!(
                            "<mrow><mi mathvariant=\"normal\">{name}</mi><mo>&#x2061;</mo><mo>(</mo>{a}<mo>)</mo></mrow>"
                        ),
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
    pub fn approximate_mathml(&self, id: Id) -> String {
        fn go(g: &Graph, id: Id, depth: usize) -> Result<String> {
            if depth > 64 {
                return Err(Error::Limit);
            }
            let f = |id| go(g, id, depth + 1);
            Ok(match &g.nodes[id] {
                Node::Matrix { cols, entries, .. } => {
                    let mut rows = String::new();
                    for row in entries.chunks(*cols) {
                        rows.push_str("<mtr>");
                        for e in row {
                            rows.push_str(&format!("<mtd>{}</mtd>", f(*e)?));
                        }
                        rows.push_str("</mtr>");
                    }
                    format!(
                        "<mrow><mo>[</mo><mtable columnspacing=\"1em\" rowspacing=\"0.45em\">{rows}</mtable><mo>]</mo></mrow>"
                    )
                }
                Node::Equation(a, b) => format!("<mrow>{}<mo>=</mo>{}</mrow>", f(*a)?, f(*b)?),
                Node::Solutions { variable, values } => format!(
                    "<mrow>{}<mo>∈</mo><mo>{{</mo>{}<mo>}}</mo></mrow>",
                    symbol_math(variable),
                    values
                        .iter()
                        .map(|n| f(*n))
                        .collect::<Result<Vec<_>>>()?
                        .join("<mo>,</mo>")
                ),
                _ => {
                    let n = g.value(id).and_then(|v| v.to_f64()).ok_or(Error::Limit)?;
                    format!("<mn>{n}</mn>")
                }
            })
        }
        format!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\">{}</math>",
            go(self, id, 0).unwrap_or_else(|_| "<mtext>…</mtext>".into())
        )
    }
    fn structured_math(&self, id: Id, depth: usize) -> Result<String> {
        let f = |i| self.pretty_math(i, 0, depth + 1);
        Ok(match &self.nodes[id] {
            Node::Integral {
                body,
                variable,
                lower,
                upper,
            } => {
                let sign = if let (Some(l), Some(u)) = (lower, upper) {
                    format!(
                        "<msubsup><mo largeop=\"true\">∫</mo>{}{}</msubsup>",
                        f(*l)?,
                        f(*u)?
                    )
                } else {
                    "<mo largeop=\"true\">∫</mo>".into()
                };
                format!(
                    "<mrow>{sign}<mrow>{}</mrow><mspace width=\"0.22em\"/><mi mathvariant=\"normal\">d</mi>{}</mrow>",
                    f(*body)?,
                    symbol_math(variable)
                )
            }
            Node::Derivative { body, variable } => format!(
                "<mrow><mfrac><mi mathvariant=\"normal\">d</mi><mrow><mi mathvariant=\"normal\">d</mi>{}</mrow></mfrac><mo>(</mo>{}<mo>)</mo></mrow>",
                symbol_math(variable),
                f(*body)?
            ),
            Node::Aggregate {
                body,
                variable,
                lower,
                upper,
                product,
            } => format!(
                "<mrow><munderover><mo largeop=\"true\">{}</mo><mrow>{}<mo>=</mo>{}</mrow>{}</munderover>{}</mrow>",
                if *product { "∏" } else { "∑" },
                symbol_math(variable),
                f(*lower)?,
                f(*upper)?,
                f(*body)?
            ),
            Node::Matrix { cols, entries, .. } => {
                let mut rows = String::new();
                for row in entries.chunks(*cols) {
                    rows.push_str("<mtr>");
                    for e in row {
                        rows.push_str(&format!("<mtd>{}</mtd>", f(*e)?));
                    }
                    rows.push_str("</mtr>");
                }
                format!(
                    "<mrow><mo stretchy=\"true\">[</mo><mtable columnspacing=\"1em\" rowspacing=\"0.45em\">{rows}</mtable><mo stretchy=\"true\">]</mo></mrow>"
                )
            }
            Node::Equation(a, b) => format!("<mrow>{}<mo>=</mo>{}</mrow>", f(*a)?, f(*b)?),
            Node::Solutions { variable, values } => {
                let body = values
                    .iter()
                    .map(|n| f(*n))
                    .collect::<Result<Vec<_>>>()?
                    .join("<mo>,</mo><mspace width=\"0.2em\"/>");
                if values.is_empty() {
                    format!("<mrow>{}<mo>∈</mo><mi>∅</mi></mrow>", symbol_math(variable))
                } else {
                    format!(
                        "<mrow>{}<mo>∈</mo><mo>{{</mo>{body}<mo>}}</mo></mrow>",
                        symbol_math(variable)
                    )
                }
            }
            _ => return Err(Error::Invalid("structured display".into())),
        })
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
                Node::Constant(s) => {
                    if s == "pi" {
                        std::f64::consts::PI
                    } else {
                        std::f64::consts::E
                    }
                }
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
                    "floor" => values[a].floor(),
                    "ceil" => values[a].ceil(),
                    _ => return Err(Error::Unsupported(f.clone())),
                },
                _ => {
                    return Err(Error::Unsupported(
                        "reduce structured expression before numeric evaluation".into(),
                    ));
                }
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

pub fn symbol_math(s: &str) -> String {
    let glyph = match s {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "theta" => "θ",
        "lambda" => "λ",
        "mu" => "μ",
        "sigma" => "σ",
        "omega" => "ω",
        _ => s,
    };
    if let Some((a, b)) = s
        .split_once('_')
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
    {
        format!("<msub><mi>{a}</mi><mi>{b}</mi></msub>")
    } else {
        format!("<mi>{glyph}</mi>")
    }
}
