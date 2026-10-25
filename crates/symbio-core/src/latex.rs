//! Bounded mathematical LaTeX, not a TeX interpreter. No macros, HTML or file I/O.
//! All accepted notation constructs the same immutable SEG as the expression parser.
use crate::{
    Error, Result,
    graph::{Graph, Id, Node},
    parser,
};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(String),
    Symbol(String),
    Reference(String),
    Command(String),
    Op(char),
    Row,
    End,
}

#[derive(Serialize)]
pub struct Preview {
    pub expression: String,
    pub mathml: String,
    pub symbols: Vec<String>,
    pub kind: String,
    pub parameters: Vec<String>,
    pub dependent_symbol: Option<String>,
}
pub fn preview(source: &str) -> Result<Preview> {
    let mut graph = Graph::default();
    let root = parse(&mut graph, source)?;
    if crate::dependencies::references(&graph, root).is_empty() {
        crate::structured::shape(&graph, root)?;
    }
    Ok(Preview {
        parameters: graph.parameters(root),
        dependent_symbol: graph.dependent_symbol(root),
        kind: crate::structured::kind(&graph, root).into(),
        expression: graph.text(root),
        mathml: graph.mathml(root),
        symbols: graph.symbols(root).into_iter().collect(),
    })
}

pub fn parse(graph: &mut Graph, source: &str) -> Result<Id> {
    if source.len() > 2048 {
        return Err(Error::Limit);
    }
    let mut source = source.trim();
    for (open, close) in [("$$", "$$"), ("$", "$"), (r"\(", r"\)"), (r"\[", r"\]")] {
        if source.starts_with(open) {
            if source.len() < open.len() + close.len() || !source.ends_with(close) {
                return Err(Error::Parse(0));
            }
            source = &source[open.len()..source.len() - close.len()];
            break;
        }
    }
    let chars: Vec<_> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        let token = match c {
            c if c.is_whitespace() => continue,
            '\\' => {
                if i >= chars.len() {
                    return Err(Error::Parse(i));
                }
                if chars[i] == '\\' {
                    i += 1;
                    tokens.push(Token::Row);
                    continue;
                }
                if ['{', '}'].contains(&chars[i]) {
                    tokens.push(Token::Op(chars[i]));
                    i += 1;
                    continue;
                }
                if [',', ';', ':', '!', ' '].contains(&chars[i]) {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                if start == i {
                    return Err(Error::Parse(i));
                }
                let name: String = chars[start..i].iter().collect();
                match name.as_str() {
                    "quad" | "qquad" | "displaystyle" => continue,
                    "cell" => {
                        if chars.get(i) != Some(&'{') {
                            return Err(Error::Parse(i));
                        }
                        i += 1;
                        let start = i;
                        while i < chars.len() && chars[i] != '}' {
                            i += 1;
                        }
                        let name: String = chars[start..i].iter().collect();
                        if chars.get(i) != Some(&'}') || !crate::graph::valid_symbol(&name) {
                            return Err(Error::Parse(i));
                        }
                        i += 1;
                        Token::Reference(name)
                    }
                    "mathit" => {
                        if chars.get(i) != Some(&'{') {
                            return Err(Error::Parse(i));
                        }
                        i += 1;
                        let start = i;
                        while i < chars.len() && chars[i] != '}' {
                            i += 1;
                        }
                        let name: String = chars[start..i].iter().collect();
                        if chars.get(i) != Some(&'}') || !crate::graph::valid_symbol(&name) {
                            return Err(Error::Parse(i));
                        }
                        i += 1;
                        Token::Symbol(name)
                    }
                    "mathrm" => {
                        if chars.get(i..i + 3) == Some(&['{', 'd', '}']) {
                            i += 3;
                            Token::Symbol("d".into())
                        } else {
                            return Err(Error::Unsupported(
                                "LaTeX \\mathrm: differential d only".into(),
                            ));
                        }
                    }
                    "partial" => Token::Symbol("d".into()),
                    "pi" => Token::Command("pi".into()),
                    "top" => Token::Symbol("T".into()),
                    "int" | "sum" | "prod" | "begin" | "end" | "det" | "lfloor" | "rfloor"
                    | "lceil" | "rceil" => Token::Command(name),
                    "cdot" | "times" => Token::Op('*'),
                    "div" => Token::Op('/'),
                    "frac" | "dfrac" | "tfrac" | "sqrt" | "sin" | "cos" | "exp" | "ln" | "log"
                    | "left" | "right" | "lvert" | "rvert" => Token::Command(name),
                    // Greek letters are variables, never implicit physical or numeric constants.
                    "alpha" | "beta" | "gamma" | "delta" | "theta" | "lambda" | "mu" | "sigma"
                    | "omega" => Token::Symbol(name),
                    _ => return Err(Error::Unsupported(format!("LaTeX \\{name}"))),
                }
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i - 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                Token::Number(chars[start..i].iter().collect())
            }
            c if c.is_ascii_alphabetic() => Token::Symbol(c.to_string()),
            c if "+-*/^(){}[]|_=&".contains(c) => Token::Op(c),
            '−' => Token::Op('-'),
            '×' | '·' => Token::Op('*'),
            _ => return Err(Error::Parse(i - 1)),
        };
        tokens.push(token);
        if tokens.len() > 512 {
            return Err(Error::Limit);
        }
    }
    if tokens.len() > 512 {
        return Err(Error::Limit);
    }
    tokens.push(Token::End);
    let mut p = Parser {
        graph,
        tokens,
        pos: 0,
        level: 0,
        integral_stops: Vec::new(),
    };
    let root = p.expression(0, 0)?;
    if p.peek() != &Token::End {
        return Err(Error::Parse(p.pos));
    }
    Ok(root)
}
struct Parser<'a> {
    graph: &'a mut Graph,
    tokens: Vec<Token>,
    pos: usize,
    level: usize,
    integral_stops: Vec<usize>,
}
impl Parser<'_> {
    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::End)
    }
    fn take(&mut self) -> Token {
        let t = self.peek().clone();
        self.pos += 1;
        t
    }
    fn expect(&mut self, expected: Token) -> Result<()> {
        if self.peek() != &expected {
            return Err(Error::Parse(self.pos));
        }
        self.pos += 1;
        Ok(())
    }
    fn group(&mut self, depth: usize) -> Result<Id> {
        self.expect(Token::Op('{'))?;
        self.level += 1;
        let root = self.expression(0, depth + 1)?;
        self.level -= 1;
        self.expect(Token::Op('}'))?;
        Ok(root)
    }
    fn exponent(&mut self, depth: usize) -> Result<Id> {
        if self.peek() == &Token::Op('{') {
            return self.group(depth);
        }
        // TeX's unbraced exponent is one token: refuse ambiguous multi-digit input.
        match self.peek() {
            Token::Number(s) if s.len() == 1 => {}
            Token::Symbol(_) => {}
            _ => return Err(Error::Unsupported("LaTeX exponent: use ^{...}".into())),
        }
        self.atom(depth + 1)
    }
    fn atom(&mut self, depth: usize) -> Result<Id> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        match self.take() {
            Token::Number(s) => parser::parse(self.graph, &s),
            Token::Reference(s) => self.graph.intern(Node::Reference(s)),
            Token::Symbol(mut s) => {
                if self.peek() == &Token::Op('_') {
                    self.take();
                    let grouped = self.peek() == &Token::Op('{');
                    if grouped {
                        self.take();
                    }
                    let index = match self.take() {
                        Token::Symbol(v) | Token::Number(v)
                            if v.chars().all(|c| c.is_ascii_alphanumeric()) =>
                        {
                            v
                        }
                        _ => return Err(Error::Parse(self.pos)),
                    };
                    if grouped {
                        self.expect(Token::Op('}'))?;
                    }
                    s = format!("{s}_{index}");
                }
                if !crate::graph::valid_symbol(&s) {
                    return Err(Error::Parse(self.pos));
                }
                self.graph.intern(Node::Symbol(s))
            }
            Token::Op(open @ ('(' | '{' | '[')) => {
                self.level += 1;
                let a = self.expression(0, depth + 1)?;
                self.level -= 1;
                self.expect(Token::Op(match open {
                    '(' => ')',
                    '{' => '}',
                    _ => ']',
                }))?;
                Ok(a)
            }
            Token::Op('-') => {
                let b = self.expression(5, depth + 1)?;
                let a = self.graph.number(-1)?;
                self.graph.intern(Node::Mul(a, b))
            }
            Token::Op('+') => self.expression(5, depth + 1),
            Token::Command(s) => match s.as_str() {
                "pi" => self.graph.intern(Node::Constant("pi".into())),
                "int" => self.integral(depth),
                "sum" | "prod" => self.aggregate(depth, s == "prod"),
                "begin" => self.matrix(depth),
                "lfloor" | "lceil" => {
                    let a = self.expression(0, depth + 1)?;
                    self.expect(Token::Command(
                        if s == "lfloor" { "rfloor" } else { "rceil" }.into(),
                    ))?;
                    self.graph.intern(Node::Call(
                        if s == "lfloor" { "floor" } else { "ceil" }.into(),
                        a,
                    ))
                }
                "frac" | "dfrac" | "tfrac" => {
                    if self.tokens.get(self.pos..self.pos+7).is_some_and(|t| matches!(t, [Token::Op('{'),Token::Symbol(d),Token::Op('}'),Token::Op('{'),Token::Symbol(dd),Token::Symbol(_),Token::Op('}')] if d=="d"&&dd=="d")) {
                        let Token::Symbol(variable)=self.tokens[self.pos+5].clone() else { unreachable!() };
                        self.pos+=7; let body=self.expression(0,depth+1)?;
                        return self.graph.intern(Node::Derivative { body, variable });
                    }
                    let a = self.group(depth)?;
                    let b = self.group(depth)?;
                    self.graph.intern(Node::Div(a, b))
                }
                "sqrt" => {
                    let a = self.group(depth)?;
                    self.graph.intern(Node::Call("sqrt".into(), a))
                }
                "sin" | "cos" | "exp" | "ln" | "log" | "det" => {
                    let power = if self.peek() == &Token::Op('^') {
                        self.take();
                        Some(self.exponent(depth)?)
                    } else {
                        None
                    };
                    // Explicit groups avoid the ambiguity of sin x y and implicit products.
                    let grouped = matches!(self.peek(), Token::Op('(' | '{'))
                        || self.peek() == &Token::Command("left".into());
                    if !grouped {
                        return Err(Error::Unsupported(
                            "LaTeX function: use \\sin(x) or \\sin{x}".into(),
                        ));
                    }
                    let a = self.atom(depth + 1)?;
                    let a = self
                        .graph
                        .intern(Node::Call(if s == "ln" { "log".into() } else { s }, a))?;
                    if let Some(b) = power {
                        self.graph.intern(Node::Pow(a, b))
                    } else {
                        Ok(a)
                    }
                }
                "left" => {
                    let close = match self.take() {
                        Token::Op('(') => ')',
                        Token::Op('[') => ']',
                        Token::Op('|') => '|',
                        Token::Op('{') => '}',
                        Token::Command(s) if s == "lfloor" => '⌋',
                        Token::Command(s) if s == "lceil" => '⌉',
                        _ => return Err(Error::Parse(self.pos)),
                    };
                    self.level += 1;
                    let a = self.expression(0, depth + 1)?;
                    self.level -= 1;
                    self.expect(Token::Command("right".into()))?;
                    self.expect(if close == '⌋' {
                        Token::Command("rfloor".into())
                    } else if close == '⌉' {
                        Token::Command("rceil".into())
                    } else {
                        Token::Op(close)
                    })?;
                    if close == '⌋' || close == '⌉' {
                        return self.graph.intern(Node::Call(
                            if close == '⌋' { "floor" } else { "ceil" }.into(),
                            a,
                        ));
                    }
                    if close == '|' {
                        self.graph.intern(Node::Call("abs".into(), a))
                    } else {
                        Ok(a)
                    }
                }
                "lvert" => {
                    let a = self.expression(0, depth + 1)?;
                    self.expect(Token::Command("rvert".into()))?;
                    self.graph.intern(Node::Call("abs".into(), a))
                }
                _ => Err(Error::Parse(self.pos)),
            },
            _ => Err(Error::Parse(self.pos)),
        }
    }
    fn differential(&self) -> Option<String> {
        match self.tokens.get(self.pos..self.pos + 2)? {
            [Token::Symbol(d), Token::Symbol(v)] if d == "d" => Some(v.clone()),
            _ => None,
        }
    }
    fn limits(&mut self, depth: usize) -> Result<(Option<Id>, Option<Id>)> {
        let (mut lower, mut upper) = (None, None);
        while matches!(self.peek(), Token::Op('_' | '^')) {
            let lower_bound = self.take() == Token::Op('_');
            let value = self.exponent(depth + 1)?;
            let slot = if lower_bound { &mut lower } else { &mut upper };
            if slot.replace(value).is_some() {
                return Err(Error::Parse(self.pos));
            }
        }
        Ok((lower, upper))
    }
    fn integral(&mut self, depth: usize) -> Result<Id> {
        let (lower, upper) = self.limits(depth)?;
        if lower.is_some() != upper.is_some() {
            return Err(Error::Parse(self.pos));
        }
        self.integral_stops.push(self.level);
        let body = self.expression(0, depth + 1)?;
        self.integral_stops.pop();
        let variable = self.differential().ok_or_else(|| {
            Error::Unsupported("integral requires a differential, e.g. dx".into())
        })?;
        self.pos += 2;
        self.graph.intern(Node::Integral {
            body,
            variable,
            lower,
            upper,
        })
    }
    fn aggregate(&mut self, depth: usize, product: bool) -> Result<Id> {
        let (lower, upper) = self.limits(depth)?;
        let (Some(lower), Some(upper)) = (lower, upper) else {
            return Err(Error::Parse(self.pos));
        };
        let Node::Equation(symbol, lower) = self.graph.nodes[lower].clone() else {
            return Err(Error::Unsupported("sum/product requires _{i=1}^{n}".into()));
        };
        let Node::Symbol(variable) = self.graph.nodes[symbol].clone() else {
            return Err(Error::Parse(self.pos));
        };
        let body = self.expression(3, depth + 1)?;
        self.graph.intern(Node::Aggregate {
            body,
            variable,
            lower,
            upper,
            product,
        })
    }
    fn environment(&mut self) -> Result<String> {
        self.expect(Token::Op('{'))?;
        let mut name = String::new();
        while let Token::Symbol(s) = self.peek() {
            name.push_str(s);
            self.take();
        }
        self.expect(Token::Op('}'))?;
        if !["matrix", "pmatrix", "bmatrix", "Bmatrix", "vmatrix"].contains(&name.as_str()) {
            return Err(Error::Unsupported(format!("matrix environment {name}")));
        }
        Ok(name)
    }
    fn matrix(&mut self, depth: usize) -> Result<Id> {
        let env = self.environment()?;
        self.level += 1;
        let mut entries = Vec::new();
        let mut widths = Vec::new();
        let mut width = 0;
        loop {
            entries.push(self.expression(0, depth + 1)?);
            width += 1;
            if entries.len() > 64 || width > 8 {
                return Err(Error::Limit);
            }
            match self.take() {
                Token::Op('&') => {}
                Token::Row => {
                    widths.push(width);
                    width = 0;
                    if widths.len() >= 8 {
                        return Err(Error::Limit);
                    }
                }
                Token::Command(s) if s == "end" => {
                    widths.push(width);
                    break;
                }
                _ => return Err(Error::Parse(self.pos)),
            }
        }
        self.level -= 1;
        if self.environment()? != env || widths.iter().any(|n| *n != width) {
            return Err(Error::Invalid(
                "matrix rows must have equal lengths and matching environments".into(),
            ));
        }
        let root = self.graph.intern(Node::Matrix {
            rows: widths.len(),
            cols: width,
            entries,
        })?;
        if env == "vmatrix" {
            self.graph.intern(Node::Call("det".into(), root))
        } else {
            Ok(root)
        }
    }
    fn expression(&mut self, min: u8, depth: usize) -> Result<Id> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        let mut a = self.atom(depth + 1)?;
        loop {
            if self.integral_stops.last() == Some(&self.level) && self.differential().is_some() {
                break;
            }
            let (op, l, r, implicit) = match self.peek() {
                Token::Op('=') => ('=', 0, 1, false),
                Token::Op('+') => ('+', 1, 2, false),
                Token::Op('-') => ('-', 1, 2, false),
                Token::Op('*') => ('*', 3, 4, false),
                Token::Op('/') => ('/', 3, 4, false),
                Token::Op('^') => ('^', 7, 6, false),
                Token::Number(_)
                | Token::Symbol(_)
                | Token::Reference(_)
                | Token::Op('(' | '{' | '[') => ('*', 3, 4, true),
                Token::Command(s)
                    if !["right", "rvert", "end", "rfloor", "rceil"].contains(&s.as_str()) =>
                {
                    ('*', 3, 4, true)
                }
                _ => break,
            };
            if l < min {
                break;
            }
            if !implicit {
                self.take();
            }
            let b = if op == '^' {
                self.exponent(depth + 1)?
            } else {
                self.expression(r, depth + 1)?
            };
            a = self.graph.intern(match op {
                '+' => Node::Add(a, b),
                '-' => Node::Sub(a, b),
                '*' => Node::Mul(a, b),
                '/' => Node::Div(a, b),
                '=' => Node::Equation(a, b),
                _ if matches!(&self.graph.nodes[b],Node::Symbol(s) if s=="T")
                    && matches!(
                        crate::structured::shape(self.graph, a)?,
                        crate::structured::Shape::Matrix(..)
                    ) =>
                {
                    Node::Call("transpose".into(), a)
                }
                _ => Node::Pow(a, b),
            })?;
            if op == '^' && self.peek() == &Token::Op('^') {
                return Err(Error::Parse(self.pos));
            }
        }
        Ok(a)
    }
}

/// A format switch preserves mathematical meaning, or fails without changing a draft.
pub fn convert(source: &str, from: crate::SourceFormat, to: crate::SourceFormat) -> Result<String> {
    let mut g = Graph::default();
    let root = from.parse(&mut g, source)?;
    if crate::dependencies::references(&g, root).is_empty() {
        crate::structured::shape(&g, root)?;
    }
    if from == to {
        return Ok(source.into());
    }
    if to == crate::SourceFormat::Latex {
        return tex(&g, root, 0);
    }
    // The ordinary parser has no matrix, binder, equation or numeric-constant syntax.
    if g.nodes.iter().any(|n|!matches!(n,Node::Number(_)|Node::Symbol(_)|Node::Reference(_)|Node::Add(..)|Node::Sub(..)|Node::Mul(..)|Node::Div(..)|Node::Pow(..)|Node::Call(..))) || g.nodes.iter().any(|n|matches!(n,Node::Call(f,_) if !["sin","cos","exp","log","abs","sqrt"].contains(&f.as_str()))) {return Err(Error::Unsupported("this expression requires LaTeX mode".into()));}
    let out = g.text(root);
    if out.len() > 2048 {
        return Err(Error::Limit);
    }
    Ok(out)
}

/// Serialize a resolved definition for source exchange with external editors.
pub fn source_from_graph(g: &Graph, root: Id) -> Result<String> {
    tex(g, root, 0)
}
fn tex(g: &Graph, id: Id, depth: usize) -> Result<String> {
    if depth > 64 {
        return Err(Error::Limit);
    }
    let f = |id| tex(g, id, depth + 1);
    let out = match &g.nodes[id] {
        Node::Number(n) => {
            if let Some((a, b)) = n.split_once('/') {
                format!(r"\frac{{{a}}}{{{b}}}")
            } else {
                n.clone()
            }
        }
        Node::Constant(s) => {
            if s == "pi" {
                "\\pi".into()
            } else {
                "\\exp(1)".into()
            }
        }
        Node::Matrix { cols, entries, .. } => {
            let rows = entries
                .chunks(*cols)
                .map(|row| {
                    row.iter()
                        .map(|id| f(*id))
                        .collect::<Result<Vec<_>>>()
                        .map(|row| row.join(" & "))
                })
                .collect::<Result<Vec<_>>>()?;
            format!("\\begin{{bmatrix}}{}\\end{{bmatrix}}", rows.join(" \\ "))
        }
        Node::Equation(a, b) => format!("{}={}", f(*a)?, f(*b)?),
        Node::Integral {
            body,
            variable,
            lower,
            upper,
        } => {
            let limits = match (lower, upper) {
                (Some(l), Some(u)) => format!("_{{{}}}^{{{}}}", f(*l)?, f(*u)?),
                _ => String::new(),
            };
            format!("\\int{limits}{{{}}}\\,d\\mathit{{{variable}}}", f(*body)?)
        }
        Node::Derivative { body, variable } => format!(
            "\\frac{{d}}{{d\\mathit{{{variable}}}}}\\left({}\\right)",
            f(*body)?
        ),
        Node::Aggregate {
            body,
            variable,
            lower,
            upper,
            product,
        } => format!(
            "\\{}_{{\\mathit{{{variable}}}={}}}^{{{}}}{{{}}}",
            if *product { "prod" } else { "sum" },
            f(*lower)?,
            f(*upper)?,
            f(*body)?
        ),
        Node::Reference(s) => format!("\\cell{{{s}}}"),
        Node::Symbol(s) => {
            if s.len() == 1 {
                s.clone()
            } else {
                format!(r"\mathit{{{s}}}")
            }
        }
        Node::Add(a, b) => format!(r"\left({}+{}\right)", f(*a)?, f(*b)?),
        Node::Sub(a, b) => format!(r"\left({}-{}\right)", f(*a)?, f(*b)?),
        Node::Mul(a, b) => format!(r"\left({}\cdot {}\right)", f(*a)?, f(*b)?),
        Node::Div(a, b) => format!(r"\frac{{{}}}{{{}}}", f(*a)?, f(*b)?),
        Node::Pow(a, b) => format!(r"\left({}\right)^{{{}}}", f(*a)?, f(*b)?),
        Node::Call(name, a) => match name.as_str() {
            "sqrt" => format!(r"\sqrt{{{}}}", f(*a)?),
            "abs" => format!(r"\left|{}\right|", f(*a)?),
            "transpose" => format!(r"\left({}\right)^{{T}}", f(*a)?),
            "inverse" => format!(r"\left({}\right)^{{-1}}", f(*a)?),
            "floor" => format!(r"\lfloor {}\rfloor", f(*a)?),
            "ceil" => format!(r"\lceil {}\rceil", f(*a)?),
            _ => format!(
                r"\{}\left({}\right)",
                if name == "log" { "ln" } else { name },
                f(*a)?
            ),
        },
        _ => {
            return Err(Error::Unsupported(
                "use LaTeX for this structured expression".into(),
            ));
        }
    };
    if out.len() > 2048 {
        return Err(Error::Limit);
    }
    Ok(out)
}
