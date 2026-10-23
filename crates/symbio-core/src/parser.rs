use crate::{
    Error, Result,
    graph::{Graph, Id, Node, Rational},
};
use num_bigint::BigInt;
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(String),
    Name(String),
    Op(char),
    End,
}
pub fn parse(graph: &mut Graph, source: &str) -> Result<Id> {
    if source.len() > 2048 {
        return Err(Error::Limit);
    }
    let mut tokens = vec![];
    let chars: Vec<_> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            let mut dots = 0;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                if chars[i] == '.' {
                    dots += 1
                }
                i += 1;
            }
            if dots > 1 || i - start > 256 {
                return Err(Error::Parse(start));
            }
            tokens.push(Token::Number(chars[start..i].iter().collect()));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1
            }
            tokens.push(Token::Name(chars[start..i].iter().collect()));
        } else if "+-*/^()".contains(c) {
            tokens.push(Token::Op(c));
            i += 1
        } else {
            return Err(Error::Parse(i));
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
    };
    let root = p.expression(0, 0)?;
    if p.tokens[p.pos] != Token::End {
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
    fn expression(&mut self, min: u8, depth: usize) -> Result<Id> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        let token = self.tokens[self.pos].clone();
        self.pos += 1;
        let mut a = match token {
            Token::Number(s) => {
                let (whole, frac) = s.split_once('.').unwrap_or((&s, ""));
                let digits = format!("{whole}{frac}");
                let n = digits
                    .parse::<BigInt>()
                    .map_err(|_| Error::Parse(self.pos))?;
                self.graph
                    .rat(Rational::new(n, BigInt::from(10u8).pow(frac.len() as u32)))?
            }
            Token::Name(s) => {
                if !crate::graph::valid_symbol(&s) {
                    return Err(Error::Parse(self.pos));
                }
                if self.tokens[self.pos] == Token::Op('(') {
                    if !["sin", "cos", "exp", "log", "sqrt", "abs"].contains(&s.as_str()) {
                        return Err(Error::Unsupported(s));
                    }
                    self.pos += 1;
                    let arg = self.expression(0, depth + 1)?;
                    self.close()?;
                    self.graph.intern(Node::Call(s, arg))?
                } else {
                    self.graph.intern(Node::Symbol(s))?
                }
            }
            Token::Op('(') => {
                let x = self.expression(0, depth + 1)?;
                self.close()?;
                x
            }
            Token::Op('-') => {
                let b = self.expression(5, depth + 1)?;
                let a = self.graph.number(-1)?;
                self.graph.intern(Node::Mul(a, b))?
            }
            Token::Op('+') => self.expression(5, depth + 1)?,
            _ => return Err(Error::Parse(self.pos)),
        };
        loop {
            let (op, l, r) = match self.tokens[self.pos] {
                Token::Op('+') => ('+', 1, 2),
                Token::Op('-') => ('-', 1, 2),
                Token::Op('*') => ('*', 3, 4),
                Token::Op('/') => ('/', 3, 4),
                Token::Op('^') => ('^', 7, 6),
                _ => break,
            };
            if l < min {
                break;
            }
            self.pos += 1;
            let b = self.expression(r, depth + 1)?;
            a = self.graph.intern(match op {
                '+' => Node::Add(a, b),
                '-' => Node::Sub(a, b),
                '*' => Node::Mul(a, b),
                '/' => Node::Div(a, b),
                _ => Node::Pow(a, b),
            })?;
        }
        Ok(a)
    }
    fn close(&mut self) -> Result<()> {
        if self.tokens[self.pos] != Token::Op(')') {
            return Err(Error::Parse(self.pos));
        }
        self.pos += 1;
        Ok(())
    }
}
