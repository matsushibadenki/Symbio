use crate::{
    Error, Result,
    graph::{Graph, Id, Node, Rational},
    model::{Assumption, Domain},
};
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::collections::{BTreeMap, HashMap};
pub type Assumptions = BTreeMap<String, Assumption>;
fn nonzero(g: &Graph, id: Id, a: &Assumptions) -> bool {
    g.value(id).is_some_and(|v| !v.is_zero())
        || matches!(&g.nodes[id],Node::Symbol(s) if a.get(s).is_some_and(|v|v.nonzero||v.domain==Domain::Positive))
}
fn total(g: &Graph, id: Id) -> bool {
    match &g.nodes[id] {
        Node::Number(_) | Node::Symbol(_) | Node::Constant(_) => true,
        Node::Add(a, b) | Node::Sub(a, b) | Node::Mul(a, b) => total(g, *a) && total(g, *b),
        Node::Pow(a, b) => {
            g.value(*b)
                .is_some_and(|v| v.is_integer() && v.is_positive())
                && total(g, *a)
        }
        _ => false,
    }
}
pub fn simplify(g: &mut Graph, root: Id, a: &Assumptions) -> Result<Id> {
    fn go(
        g: &mut Graph,
        id: Id,
        a: &Assumptions,
        memo: &mut HashMap<Id, Id>,
        depth: usize,
    ) -> Result<Id> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        if let Some(v) = memo.get(&id) {
            return Ok(*v);
        }
        let n = g.nodes[id].clone();
        if matches!(n, Node::Reference(_)) {
            return Err(Error::Invalid("unresolved cell reference".into()));
        }
        if matches!(
            n,
            Node::Constant(_)
                | Node::Integral { .. }
                | Node::Derivative { .. }
                | Node::Aggregate { .. }
                | Node::Matrix { .. }
                | Node::Equation(..)
                | Node::Solutions { .. }
        ) {
            let result = crate::structured::reduce(g, id, a)?;
            memo.insert(id, result);
            return Ok(result);
        }
        let children = n.children();
        let mut out = vec![];
        for c in children {
            out.push(go(g, c, a, memo, depth + 1)?)
        }
        let result = match n {
            Node::Number(_) | Node::Symbol(_) => id,
            Node::Call(f, _) => {
                let v = out[0];
                if ["det", "inverse", "transpose"].contains(&f.as_str()) {
                    let r = crate::structured::matrix_call(g, &f, v, a)?;
                    memo.insert(id, r);
                    return Ok(r);
                }
                match (f.as_str(), g.nodes[v].clone()) {
                    ("sqrt", _) if g.value(v).is_some_and(|v| !v.is_negative()) => {
                        let q = g.value(v).unwrap();
                        let n = q.numer().sqrt();
                        let d = q.denom().sqrt();
                        if &n * &n == *q.numer() && &d * &d == *q.denom() {
                            g.rat(Rational::new(n, d))?
                        } else {
                            g.intern(Node::Call(f, v))?
                        }
                    }
                    ("sin", Node::Constant(s)) if s == "pi" => g.number(0)?,
                    ("cos", Node::Constant(s)) if s == "pi" => g.number(-1)?,
                    ("sin", _) if g.value(v).is_some_and(|v| v.is_zero()) => g.number(0)?,
                    ("cos" | "exp", _) if g.value(v).is_some_and(|v| v.is_zero()) => g.number(1)?,
                    ("floor", _) if g.value(v).is_some() => g.rat(g.value(v).unwrap().floor())?,
                    ("ceil", _) if g.value(v).is_some() => g.rat(g.value(v).unwrap().ceil())?,
                    ("sqrt", Node::Pow(base, power))
                        if g.value(power) == Some(Rational::from_integer(2.into())) =>
                    {
                        if let Node::Symbol(s) = &g.nodes[base] {
                            match a.get(s).map(|a| &a.domain) {
                                Some(Domain::Positive | Domain::Nonnegative) => base,
                                Some(Domain::Real) => g.intern(Node::Call("abs".into(), base))?,
                                _ => g.intern(Node::Call(f, v))?,
                            }
                        } else {
                            g.intern(Node::Call(f, v))?
                        }
                    }
                    ("abs", _) if g.value(v).is_some() => g.rat(g.value(v).unwrap().abs())?,
                    _ => g.intern(Node::Call(f, v))?,
                }
            }
            n => {
                let (x, y) = (out[0], out[1]);
                if let Some(result) = crate::structured::matrix_binary(g, &n, x, y, a)? {
                    memo.insert(id, result);
                    return Ok(result);
                }
                let xv = g.value(x);
                let yv = g.value(y);
                let zero = Rational::zero();
                let one = Rational::one();
                if matches!(n, Node::Div(..)) && yv == Some(zero.clone()) {
                    return Err(Error::Domain("division by zero".into()));
                }
                if let (Some(xv), Some(yv)) = (&xv, &yv) {
                    let r = match n {
                        Node::Add(..) => Some(xv + yv),
                        Node::Sub(..) => Some(xv - yv),
                        Node::Mul(..) => Some(xv * yv),
                        Node::Div(..) => Some(xv / yv),
                        Node::Pow(..) => {
                            if xv.is_zero() && yv <= &zero {
                                return Err(Error::Domain("undefined power".into()));
                            }
                            if yv.is_integer() {
                                let e = yv.to_integer().to_i32().ok_or(Error::Limit)?;
                                if e.unsigned_abs() > 64 {
                                    return Err(Error::Limit);
                                }
                                Some(xv.pow(e))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(r) = r {
                        let v = g.rat(r)?;
                        memo.insert(id, v);
                        return Ok(v);
                    }
                }
                match n {
                    Node::Add(..) if xv == Some(zero.clone()) => y,
                    Node::Add(..) | Node::Sub(..) if yv == Some(zero.clone()) => x,
                    Node::Mul(..) if xv == Some(one.clone()) => y,
                    Node::Mul(..) | Node::Div(..) if yv == Some(one.clone()) => x,
                    Node::Mul(..)
                        if (xv == Some(zero.clone()) && total(g, y))
                            || (yv == Some(zero.clone()) && total(g, x)) =>
                    {
                        g.number(0)?
                    }
                    Node::Div(..) if x == y && nonzero(g, x, a) => g.number(1)?,
                    Node::Pow(..) if yv == Some(one) => x,
                    Node::Pow(..) if yv == Some(zero) && nonzero(g, x, a) => g.number(1)?,
                    Node::Add(..) => g.intern(Node::Add(x, y))?,
                    Node::Sub(..) => g.intern(Node::Sub(x, y))?,
                    Node::Mul(..) => g.intern(Node::Mul(x, y))?,
                    Node::Div(..) => g.intern(Node::Div(x, y))?,
                    _ => g.intern(Node::Pow(x, y))?,
                }
            }
        };
        memo.insert(id, result);
        Ok(result)
    }
    let result = go(g, root, a, &mut HashMap::new(), 0)?;
    let symbols = g.symbols(result);
    if symbols.len() == 1 {
        let variable = symbols.first().unwrap();
        if let Ok(p) = polynomial(g, result, variable, 0) {
            return from_polynomial(g, &p, variable);
        }
    }
    Ok(result)
}
pub type Polynomial = BTreeMap<u32, Rational>;
fn add(mut a: Polynomial, b: Polynomial, sign: i64) -> Polynomial {
    for (p, c) in b {
        *a.entry(p).or_default() += c * Rational::from_integer(sign.into());
    }
    a.retain(|_, c| !c.is_zero());
    a
}
fn mul(a: &Polynomial, b: &Polynomial) -> Result<Polynomial> {
    let mut p = Polynomial::new();
    for (i, x) in a {
        for (j, y) in b {
            if i + j > 32 {
                return Err(Error::Limit);
            }
            *p.entry(i + j).or_default() += x * y;
        }
    }
    Ok(p)
}
pub fn polynomial(g: &Graph, id: Id, var: &str, depth: usize) -> Result<Polynomial> {
    if depth > 64 {
        return Err(Error::Limit);
    }
    let p = |i| polynomial(g, i, var, depth + 1);
    match &g.nodes[id] {
        Node::Number(s) => Ok(BTreeMap::from([(0, crate::graph::rational(s)?)])),
        Node::Symbol(s) if s == var => Ok(BTreeMap::from([(1, Rational::one())])),
        Node::Add(a, b) => Ok(add(p(*a)?, p(*b)?, 1)),
        Node::Sub(a, b) => Ok(add(p(*a)?, p(*b)?, -1)),
        Node::Mul(a, b) => mul(&p(*a)?, &p(*b)?),
        Node::Div(a, b) => {
            let d = g
                .value(*b)
                .filter(|v| !v.is_zero())
                .ok_or_else(|| Error::Unsupported("polynomial".into()))?;
            Ok(p(*a)?.into_iter().map(|(i, c)| (i, c / &d)).collect())
        }
        Node::Pow(a, b) => {
            let e = g
                .value(*b)
                .filter(|n| n.is_integer())
                .and_then(|n| n.to_u32())
                .filter(|n| *n > 0 && *n <= 32)
                .ok_or_else(|| Error::Unsupported("polynomial power".into()))?;
            let base = p(*a)?;
            let mut r = BTreeMap::from([(0, Rational::one())]);
            for _ in 0..e {
                r = mul(&r, &base)?
            }
            Ok(r)
        }
        _ => Err(Error::Unsupported("polynomial".into())),
    }
}
pub fn from_polynomial(g: &mut Graph, p: &Polynomial, var: &str) -> Result<Id> {
    let mut root = None;
    for (power, c) in p.iter().rev().filter(|(_, c)| !c.is_zero()) {
        let coefficient = g.rat(c.abs())?;
        let term = if *power == 0 {
            coefficient
        } else {
            let symbol = g.intern(Node::Symbol(var.into()))?;
            let powered = if *power == 1 {
                symbol
            } else {
                let exponent = g.number(*power as i64)?;
                g.intern(Node::Pow(symbol, exponent))?
            };
            if c.abs().is_one() {
                powered
            } else {
                g.intern(Node::Mul(coefficient, powered))?
            }
        };
        root = Some(match root {
            Some(a) => g.intern(if c.is_negative() {
                Node::Sub(a, term)
            } else {
                Node::Add(a, term)
            })?,
            None if c.is_negative() => {
                let zero = g.number(0)?;
                g.intern(Node::Sub(zero, term))?
            }
            None => term,
        });
    }
    match root {
        Some(id) => Ok(id),
        None => g.number(0),
    }
}
pub fn differentiate(g: &mut Graph, id: Id, var: &str, assumptions: &Assumptions) -> Result<Id> {
    fn go(g: &mut Graph, id: Id, var: &str, depth: usize) -> Result<Id> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        match g.nodes[id].clone() {
            Node::Number(_) => g.number(0),
            Node::Symbol(s) => g.number(i64::from(s == var)),
            Node::Add(a, b) | Node::Sub(a, b) => {
                let is_sub = matches!(g.nodes[id], Node::Sub(..));
                let da = go(g, a, var, depth + 1)?;
                let db = go(g, b, var, depth + 1)?;
                g.intern(if is_sub {
                    Node::Sub(da, db)
                } else {
                    Node::Add(da, db)
                })
            }
            Node::Mul(a, b) => {
                let da = go(g, a, var, depth + 1)?;
                let db = go(g, b, var, depth + 1)?;
                let left = g.intern(Node::Mul(da, b))?;
                let right = g.intern(Node::Mul(a, db))?;
                g.intern(Node::Add(left, right))
            }
            Node::Pow(base, exponent) => {
                let n = g
                    .value(exponent)
                    .filter(|n| n.is_integer() && n.is_positive())
                    .and_then(|n| n.to_i64())
                    .filter(|n| *n <= 32)
                    .ok_or_else(|| {
                        Error::Unsupported("differentiate: positive integer powers only".into())
                    })?;
                let d = go(g, base, var, depth + 1)?;
                if n == 1 {
                    return Ok(d);
                }
                let e = g.number(n - 1)?;
                let powered = g.intern(Node::Pow(base, e))?;
                let factor = g.intern(Node::Mul(exponent, powered))?;
                g.intern(Node::Mul(factor, d))
            }
            Node::Call(f, a) if ["sin", "cos", "exp"].contains(&f.as_str()) => {
                let da = go(g, a, var, depth + 1)?;
                let outer = g.intern(Node::Call(
                    if f == "sin" {
                        "cos"
                    } else if f == "cos" {
                        "sin"
                    } else {
                        "exp"
                    }
                    .into(),
                    a,
                ))?;
                let out = g.intern(Node::Mul(outer, da))?;
                if f == "cos" {
                    let neg = g.number(-1)?;
                    g.intern(Node::Mul(neg, out))
                } else {
                    Ok(out)
                }
            }
            Node::Div(a, b) if g.value(b).is_some_and(|v| !v.is_zero()) => {
                let d = go(g, a, var, depth + 1)?;
                g.intern(Node::Div(d, b))
            }
            _ => Err(Error::Unsupported("differentiate this expression".into())),
        }
    }
    let derivative = go(g, id, var, 0)?;
    simplify(g, derivative, assumptions)
}
pub fn integrate(g: &mut Graph, id: Id, var: &str) -> Result<Id> {
    let p = polynomial(g, id, var, 0)?;
    let integral = p
        .into_iter()
        .map(|(n, c)| (n + 1, c / Rational::from_integer((n + 1).into())))
        .collect();
    from_polynomial(g, &integral, var)
}
pub fn dimension(
    g: &Graph,
    id: Id,
    assumptions: &Assumptions,
    depth: usize,
) -> Result<Option<[i32; 3]>> {
    if depth > 64 {
        return Err(Error::Limit);
    }
    let d = |i| dimension(g, i, assumptions, depth + 1);
    let combine = |a: [i32; 3], b: [i32; 3], sign: i32| -> Result<[i32; 3]> {
        let v = std::array::from_fn(|i| a[i] + sign * b[i]);
        if v.iter().any(|n| n.abs() > 64) {
            Err(Error::Limit)
        } else {
            Ok(v)
        }
    };
    match &g.nodes[id] {
        Node::Reference(_) => Ok(None),
        Node::Constant(_) | Node::Number(_) => Ok(Some([0; 3])),
        Node::Matrix { .. }
        | Node::Integral { .. }
        | Node::Derivative { .. }
        | Node::Aggregate { .. }
        | Node::Solutions { .. } => {
            for child in g.nodes[id].children() {
                d(child)?;
            }
            Ok(None)
        }
        Node::Equation(x, y) => {
            let (x, y) = (d(*x)?, d(*y)?);
            if x.is_some() && y.is_some() && x != y {
                return Err(Error::Domain("equation dimension mismatch".into()));
            }
            Ok(None)
        }
        Node::Symbol(s) => {
            let dim = assumptions.get(s).and_then(|a| a.dimension);
            if dim.is_some_and(|d| d.iter().any(|v| v.unsigned_abs() > 16)) {
                return Err(Error::Invalid("assumption dimension".into()));
            }
            Ok(dim)
        }
        Node::Add(a, b) | Node::Sub(a, b) => {
            let (x, y) = (d(*a)?, d(*b)?);
            if x.is_some() && y.is_some() && x != y {
                return Err(Error::Domain("dimension mismatch".into()));
            }
            Ok(x.filter(|_| y.is_some()))
        }
        Node::Mul(a, b) | Node::Div(a, b) => match (d(*a)?, d(*b)?) {
            (Some(a), Some(b)) => Ok(Some(combine(
                a,
                b,
                if matches!(g.nodes[id], Node::Div(..)) {
                    -1
                } else {
                    1
                },
            )?)),
            _ => Ok(None),
        },
        Node::Pow(a, b) => {
            if d(*b)?.is_some_and(|v| v != [0; 3]) {
                return Err(Error::Domain("dimensioned exponent".into()));
            }
            match (
                d(*a)?,
                g.value(*b)
                    .filter(|n| n.is_integer())
                    .and_then(|n| n.to_i32()),
            ) {
                (Some(a), Some(n)) if n.unsigned_abs() <= 32 => {
                    let out = a.map(|v| v * n);
                    if out.iter().any(|n| n.abs() > 64) {
                        return Err(Error::Limit);
                    }
                    Ok(Some(out))
                }
                _ => Ok(None),
            }
        }
        Node::Call(f, a) => {
            let dim = d(*a)?;
            if f == "abs" {
                return Ok(dim);
            }
            if f == "sqrt" {
                return Ok(dim
                    .filter(|d| d.iter().all(|v| v % 2 == 0))
                    .map(|d| d.map(|v| v / 2)));
            }
            if dim.is_some_and(|v| v != [0; 3]) {
                return Err(Error::Domain(
                    "function requires dimensionless input".into(),
                ));
            }
            Ok(dim.map(|_| [0; 3]))
        }
    }
}
