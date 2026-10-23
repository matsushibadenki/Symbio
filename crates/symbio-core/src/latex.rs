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
    Command(String),
    Op(char),
    End,
}

#[derive(Serialize)]
pub struct Preview {
    pub expression: String,
    pub mathml: String,
    pub symbols: Vec<String>,
}
pub fn preview(source: &str) -> Result<Preview> {
    let mut graph = Graph::default();
    let root = parse(&mut graph, source)?;
    Ok(Preview {
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
                    "quad" | "qquad" => continue,
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
            c if "+-*/^(){}[]|".contains(c) => Token::Op(c),
            '−' => Token::Op('-'),
            '×' | '·' => Token::Op('*'),
            _ => return Err(Error::Parse(i - 1)),
        };
        tokens.push(token);
        if tokens.len() > 512 {
            return Err(Error::Limit);
        }
    }
    tokens.push(Token::End);
    let mut p = Parser {
        graph,
        tokens,
        pos: 0,
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
        let root = self.expression(0, depth + 1)?;
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
            Token::Symbol(s) => self.graph.intern(Node::Symbol(s)),
            Token::Op(open @ ('(' | '{' | '[')) => {
                let a = self.expression(0, depth + 1)?;
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
                "frac" | "dfrac" | "tfrac" => {
                    let a = self.group(depth)?;
                    let b = self.group(depth)?;
                    self.graph.intern(Node::Div(a, b))
                }
                "sqrt" => {
                    let a = self.group(depth)?;
                    self.graph.intern(Node::Call("sqrt".into(), a))
                }
                "sin" | "cos" | "exp" | "ln" | "log" => {
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
                        _ => return Err(Error::Parse(self.pos)),
                    };
                    let a = self.expression(0, depth + 1)?;
                    self.expect(Token::Command("right".into()))?;
                    self.expect(Token::Op(close))?;
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
    fn expression(&mut self, min: u8, depth: usize) -> Result<Id> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        let mut a = self.atom(depth + 1)?;
        loop {
            let (op, l, r, implicit) = match self.peek() {
                Token::Op('+') => ('+', 1, 2, false),
                Token::Op('-') => ('-', 1, 2, false),
                Token::Op('*') => ('*', 3, 4, false),
                Token::Op('/') => ('/', 3, 4, false),
                Token::Op('^') => ('^', 7, 6, false),
                Token::Number(_) | Token::Symbol(_) | Token::Op('(' | '{' | '[') => {
                    ('*', 3, 4, true)
                }
                Token::Command(s) if s != "right" && s != "rvert" => ('*', 3, 4, true),
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
                _ => Node::Pow(a, b),
            })?;
            if op == '^' && self.peek() == &Token::Op('^') {
                return Err(Error::Parse(self.pos));
            }
        }
        Ok(a)
    }
}
