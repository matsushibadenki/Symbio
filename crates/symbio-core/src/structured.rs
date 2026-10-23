//! Typed operators over the shared SEG. Exact algorithms are deliberately bounded.
use crate::{
    Error, Result,
    algebra::{self, Assumptions},
    graph::{Graph, Id, Node, Rational},
    model::Domain,
};
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::collections::{BTreeMap, HashMap};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Scalar,
    Matrix(usize, usize),
    Relation,
    Solutions,
}

pub fn shape(g: &Graph, root: Id) -> Result<Shape> {
    fn go(g: &Graph, id: Id, memo: &mut HashMap<Id, Shape>, depth: usize) -> Result<Shape> {
        if depth > 96 {
            return Err(Error::Limit);
        }
        if let Some(s) = memo.get(&id) {
            return Ok(*s);
        }
        let n = g
            .nodes
            .get(id)
            .ok_or_else(|| Error::Invalid("node".into()))?;
        let mut shapes = Vec::new();
        for c in n.children() {
            shapes.push(go(g, c, memo, depth + 1)?);
        }
        let scalar = || {
            if shapes.iter().all(|s| *s == Shape::Scalar) {
                Ok(Shape::Scalar)
            } else {
                Err(Error::Domain("scalar expression required".into()))
            }
        };
        let result = match n {
            Node::Number(_) | Node::Symbol(_) | Node::Reference(_) | Node::Constant(_) => {
                Shape::Scalar
            }
            Node::Matrix {
                rows,
                cols,
                entries,
            } => {
                if *rows == 0
                    || *cols == 0
                    || *rows > 8
                    || *cols > 8
                    || rows.checked_mul(*cols) != Some(entries.len())
                {
                    return Err(Error::Invalid("matrix shape".into()));
                }
                scalar()?;
                Shape::Matrix(*rows, *cols)
            }
            Node::Equation(..) => {
                if shapes[0] != shapes[1] || !matches!(shapes[0], Shape::Scalar | Shape::Matrix(..))
                {
                    return Err(Error::Domain(
                        "equation sides have incompatible types".into(),
                    ));
                }
                Shape::Relation
            }
            Node::Solutions { .. } => {
                scalar()?;
                Shape::Solutions
            }
            Node::Add(..) | Node::Sub(..) => {
                if shapes[0] != shapes[1] || !matches!(shapes[0], Shape::Scalar | Shape::Matrix(..))
                {
                    return Err(Error::Domain(
                        "addition requires equal scalar/matrix shapes".into(),
                    ));
                }
                shapes[0]
            }
            Node::Mul(..) => match (shapes[0], shapes[1]) {
                (Shape::Scalar, Shape::Scalar) => Shape::Scalar,
                (Shape::Scalar, m @ Shape::Matrix(..)) | (m @ Shape::Matrix(..), Shape::Scalar) => {
                    m
                }
                (Shape::Matrix(r, k), Shape::Matrix(l, c)) if k == l => Shape::Matrix(r, c),
                _ => {
                    return Err(Error::Domain(
                        "matrix multiplication dimensions do not match".into(),
                    ));
                }
            },
            Node::Div(..) => {
                if shapes[1] != Shape::Scalar
                    || !matches!(shapes[0], Shape::Scalar | Shape::Matrix(..))
                {
                    return Err(Error::Domain(
                        "division requires a scalar denominator".into(),
                    ));
                }
                shapes[0]
            }
            Node::Pow(a, _) => {
                if shapes[1] != Shape::Scalar {
                    return Err(Error::Domain("scalar exponent required".into()));
                }
                match shapes[0] {
                    Shape::Scalar => Shape::Scalar,
                    Shape::Matrix(r, c) if r == c => shape(g, *a)?,
                    _ => {
                        return Err(Error::Domain(
                            "matrix power requires a square matrix".into(),
                        ));
                    }
                }
            }
            Node::Call(f, _) => match (f.as_str(), shapes[0]) {
                ("transpose", Shape::Matrix(r, c)) => Shape::Matrix(c, r),
                ("inverse", Shape::Matrix(r, c)) if r == c => Shape::Matrix(r, c),
                ("det", Shape::Matrix(r, c)) if r == c => Shape::Scalar,
                ("inverse" | "transpose" | "det", _) => {
                    return Err(Error::Domain(
                        "matrix operator requires a compatible matrix".into(),
                    ));
                }
                (_, Shape::Scalar) => Shape::Scalar,
                _ => {
                    return Err(Error::Domain(
                        "scalar function cannot consume a matrix".into(),
                    ));
                }
            },
            _ => scalar()?,
        };
        memo.insert(id, result);
        Ok(result)
    }
    go(g, root, &mut HashMap::new(), 0)
}

/// Rebuild with children in the same stable order as Node::children().
pub fn rebuild(n: &Node, c: &[Id]) -> Node {
    match n {
        Node::Add(..) => Node::Add(c[0], c[1]),
        Node::Sub(..) => Node::Sub(c[0], c[1]),
        Node::Mul(..) => Node::Mul(c[0], c[1]),
        Node::Div(..) => Node::Div(c[0], c[1]),
        Node::Pow(..) => Node::Pow(c[0], c[1]),
        Node::Equation(..) => Node::Equation(c[0], c[1]),
        Node::Call(f, _) => Node::Call(f.clone(), c[0]),
        Node::Matrix { rows, cols, .. } => Node::Matrix {
            rows: *rows,
            cols: *cols,
            entries: c.to_vec(),
        },
        Node::Derivative { variable, .. } => Node::Derivative {
            body: c[0],
            variable: variable.clone(),
        },
        Node::Integral {
            variable, lower, ..
        } => Node::Integral {
            body: c[0],
            variable: variable.clone(),
            lower: lower.map(|_| c[1]),
            upper: lower.map(|_| c[2]),
        },
        Node::Aggregate {
            variable, product, ..
        } => Node::Aggregate {
            body: c[0],
            variable: variable.clone(),
            lower: c[1],
            upper: c[2],
            product: *product,
        },
        Node::Solutions { variable, .. } => Node::Solutions {
            variable: variable.clone(),
            values: c.to_vec(),
        },
        _ => n.clone(),
    }
}

/// Capture-avoiding substitution: a bound dummy is never replaced in its body.
pub fn substitute(g: &mut Graph, id: Id, var: &str, value: Id) -> Result<Id> {
    fn go(g: &mut Graph, id: Id, var: &str, value: Id, depth: usize) -> Result<Id> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        let n = g.nodes[id].clone();
        if matches!(&n,Node::Symbol(s) if s==var) {
            return Ok(value);
        }
        // Reject capture rather than silently changing a nested binder's meaning.
        if let Node::Integral { variable, .. } | Node::Aggregate { variable, .. } = &n
            && variable != var
            && g.symbols(value).contains(variable)
            && g.symbols(id).contains(var)
        {
            return Err(Error::Unsupported(
                "substitution requires alpha-renaming a nested binder".into(),
            ));
        }
        let bound = match &n {
            Node::Integral {
                variable,
                lower: Some(_),
                ..
            }
            | Node::Aggregate { variable, .. } => Some(variable.as_str()),
            _ => None,
        };
        let mut cs = Vec::new();
        for (i, c) in n.children().into_iter().enumerate() {
            cs.push(if i == 0 && bound == Some(var) {
                c
            } else {
                go(g, c, var, value, depth + 1)?
            });
        }
        g.intern(rebuild(&n, &cs))
    }
    go(g, id, var, value, 0)
}

pub fn reduce(g: &mut Graph, id: Id, a: &Assumptions) -> Result<Id> {
    match g.nodes[id].clone() {
        Node::Constant(_) => Ok(id),
        Node::Matrix {
            rows,
            cols,
            entries,
        } => {
            let entries = entries
                .into_iter()
                .map(|e| algebra::simplify(g, e, a))
                .collect::<Result<_>>()?;
            g.intern(Node::Matrix {
                rows,
                cols,
                entries,
            })
        }
        Node::Equation(x, y) => {
            let x = algebra::simplify(g, x, a)?;
            let y = algebra::simplify(g, y, a)?;
            g.intern(Node::Equation(x, y))
        }
        Node::Solutions { .. } => Ok(id),
        Node::Derivative { body, variable } => {
            let body = algebra::simplify(g, body, a)?;
            algebra::differentiate(g, body, &variable, a)
        }
        Node::Integral {
            body,
            variable,
            lower,
            upper,
        } => {
            let mut local = a.clone();
            local.remove(&variable);
            let body = algebra::simplify(g, body, &local)?;
            let primitive = primitive(g, body, &variable)?;
            if let (Some(l), Some(u)) = (lower, upper) {
                let l = algebra::simplify(g, l, a)?;
                let u = algebra::simplify(g, u, a)?;
                let left = substitute(g, primitive, &variable, l)?;
                let right = substitute(g, primitive, &variable, u)?;
                let result = g.intern(Node::Sub(right, left))?;
                algebra::simplify(g, result, a)
            } else {
                algebra::simplify(g, primitive, a)
            }
        }
        Node::Aggregate {
            body,
            variable,
            lower,
            upper,
            product,
        } => {
            let l = algebra::simplify(g, lower, a)?;
            let u = algebra::simplify(g, upper, a)?;
            let integer = |g: &Graph, id| {
                g.value(id)
                    .filter(|v| v.is_integer())
                    .and_then(|v| v.to_i64())
                    .ok_or_else(|| {
                        Error::Unsupported("finite integer sum/product bounds required".into())
                    })
            };
            let (l, u) = (integer(g, l)?, integer(g, u)?);
            if i128::from(u) - i128::from(l) > 255 {
                return Err(Error::Limit);
            }
            let mut result = g.number(if product { 1 } else { 0 })?;
            for i in l..=u {
                let v = g.number(i)?;
                let term = substitute(g, body, &variable, v)?;
                let term = algebra::simplify(g, term, a)?;
                let n = g.intern(if product {
                    Node::Mul(result, term)
                } else {
                    Node::Add(result, term)
                })?;
                result = algebra::simplify(g, n, a)?;
            }
            Ok(result)
        }
        _ => Err(Error::Unsupported("structured operator".into())),
    }
}

fn primitive(g: &mut Graph, id: Id, var: &str) -> Result<Id> {
    if let Ok(root) = algebra::integrate(g, id, var) {
        return Ok(root);
    }
    match g.nodes[id].clone() {
        Node::Call(f, arg)
            if matches!(&g.nodes[arg],Node::Symbol(s) if s==var)
                && ["sin", "cos", "exp"].contains(&f.as_str()) =>
        {
            let root = g.intern(Node::Call(
                if f == "sin" {
                    "cos"
                } else if f == "cos" {
                    "sin"
                } else {
                    "exp"
                }
                .into(),
                arg,
            ))?;
            if f == "sin" {
                let minus = g.number(-1)?;
                g.intern(Node::Mul(minus, root))
            } else {
                Ok(root)
            }
        }
        Node::Add(x, y) | Node::Sub(x, y) => {
            let sub = matches!(g.nodes[id], Node::Sub(..));
            let x = primitive(g, x, var)?;
            let y = primitive(g, y, var)?;
            g.intern(if sub {
                Node::Sub(x, y)
            } else {
                Node::Add(x, y)
            })
        }
        _ if !g.symbols(id).contains(var) => {
            let x = g.intern(Node::Symbol(var.into()))?;
            g.intern(Node::Mul(id, x))
        }
        _ => Err(Error::Unsupported(
            "integral: rational polynomial or sin(x), cos(x), exp(x)".into(),
        )),
    }
}

pub fn matrix_binary(g: &mut Graph, n: &Node, x: Id, y: Id, a: &Assumptions) -> Result<Option<Id>> {
    let left = g.nodes[x].clone();
    let right = g.nodes[y].clone();
    if !matches!(left, Node::Matrix { .. }) && !matches!(right, Node::Matrix { .. }) {
        return Ok(None);
    }
    let temp = g.intern(rebuild(n, &[x, y]))?;
    shape(g, temp)?;
    let make = |g: &mut Graph, n: Node| -> Result<Id> {
        let n = g.intern(n)?;
        algebra::simplify(g, n, a)
    };
    let result = match (n, left, right) {
        (
            Node::Add(..) | Node::Sub(..),
            Node::Matrix {
                rows,
                cols,
                entries: l,
            },
            Node::Matrix { entries: r, .. },
        ) => {
            let entries = l
                .into_iter()
                .zip(r)
                .map(|(l, r)| {
                    make(
                        g,
                        if matches!(n, Node::Add(..)) {
                            Node::Add(l, r)
                        } else {
                            Node::Sub(l, r)
                        },
                    )
                })
                .collect::<Result<_>>()?;
            g.intern(Node::Matrix {
                rows,
                cols,
                entries,
            })?
        }
        (
            Node::Mul(..),
            Node::Matrix {
                rows: r,
                cols: k,
                entries: l,
            },
            Node::Matrix {
                cols: c,
                entries: rr,
                ..
            },
        ) => {
            let mut entries = Vec::new();
            for i in 0..r {
                for j in 0..c {
                    let mut v = g.number(0)?;
                    for m in 0..k {
                        let t = make(g, Node::Mul(l[i * k + m], rr[m * c + j]))?;
                        v = make(g, Node::Add(v, t))?;
                    }
                    entries.push(v);
                }
            }
            g.intern(Node::Matrix {
                rows: r,
                cols: c,
                entries,
            })?
        }
        (
            Node::Mul(..),
            Node::Matrix {
                rows,
                cols,
                entries,
            },
            _,
        ) => {
            let entries = entries
                .into_iter()
                .map(|v| make(g, Node::Mul(v, y)))
                .collect::<Result<_>>()?;
            g.intern(Node::Matrix {
                rows,
                cols,
                entries,
            })?
        }
        (
            Node::Mul(..),
            _,
            Node::Matrix {
                rows,
                cols,
                entries,
            },
        ) => {
            let entries = entries
                .into_iter()
                .map(|v| make(g, Node::Mul(x, v)))
                .collect::<Result<_>>()?;
            g.intern(Node::Matrix {
                rows,
                cols,
                entries,
            })?
        }
        (
            Node::Div(..),
            Node::Matrix {
                rows,
                cols,
                entries,
            },
            _,
        ) => {
            let entries = entries
                .into_iter()
                .map(|v| make(g, Node::Div(v, y)))
                .collect::<Result<_>>()?;
            g.intern(Node::Matrix {
                rows,
                cols,
                entries,
            })?
        }
        (Node::Pow(..), Node::Matrix { rows, cols, .. }, _) => {
            let e = g
                .value(y)
                .filter(|v| v.is_integer())
                .and_then(|v| v.to_i32())
                .filter(|v| v.unsigned_abs() <= 16)
                .ok_or_else(|| Error::Unsupported("matrix integer powers -16..16".into()))?;
            let base = if e < 0 {
                matrix_call(g, "inverse", x, a)?
            } else {
                x
            };
            let mut entries = Vec::new();
            for i in 0..rows {
                for j in 0..cols {
                    entries.push(g.number(i64::from(i == j))?);
                }
            }
            let mut out = g.intern(Node::Matrix {
                rows,
                cols,
                entries,
            })?;
            for _ in 0..e.unsigned_abs() {
                out = matrix_binary(g, &Node::Mul(out, base), out, base, a)?.unwrap();
            }
            out
        }
        _ => return Err(Error::Domain("unsupported matrix operator".into())),
    };
    Ok(Some(result))
}

pub fn matrix_call(g: &mut Graph, f: &str, id: Id, a: &Assumptions) -> Result<Id> {
    let Node::Matrix {
        rows,
        cols,
        entries,
    } = g.nodes[id].clone()
    else {
        return Err(Error::Domain("matrix required".into()));
    };
    if f == "transpose" {
        let mut out = Vec::new();
        for j in 0..cols {
            for i in 0..rows {
                out.push(entries[i * cols + j]);
            }
        }
        return g.intern(Node::Matrix {
            rows: cols,
            cols: rows,
            entries: out,
        });
    }
    if rows != cols {
        return Err(Error::Domain("square matrix required".into()));
    }
    if f == "det" && rows <= 2 {
        if rows == 1 {
            return Ok(entries[0]);
        }
        let left = g.intern(Node::Mul(entries[0], entries[3]))?;
        let right = g.intern(Node::Mul(entries[1], entries[2]))?;
        let n = g.intern(Node::Sub(left, right))?;
        return algebra::simplify(g, n, a);
    }
    let mut values = entries
        .iter()
        .map(|id| {
            g.value(*id).ok_or_else(|| {
                Error::Unsupported("exact rational entries required for elimination".into())
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut inverse = vec![Rational::zero(); rows * rows];
    for i in 0..rows {
        inverse[i * rows + i] = Rational::one();
    }
    let mut determinant = Rational::one();
    for k in 0..rows {
        let Some(pivot) = (k..rows).find(|r| !values[r * rows + k].is_zero()) else {
            return if f == "det" {
                g.number(0)
            } else {
                Err(Error::Domain("singular matrix has no inverse".into()))
            };
        };
        if pivot != k {
            for j in 0..rows {
                values.swap(k * rows + j, pivot * rows + j);
                inverse.swap(k * rows + j, pivot * rows + j);
            }
            determinant = -determinant;
        }
        let v = values[k * rows + k].clone();
        determinant *= &v;
        for j in 0..rows {
            values[k * rows + j] /= &v;
            inverse[k * rows + j] /= &v;
        }
        for i in 0..rows {
            if i == k {
                continue;
            }
            let factor = values[i * rows + k].clone();
            for j in 0..rows {
                let t = &factor * &values[k * rows + j];
                values[i * rows + j] -= t;
                let t = &factor * &inverse[k * rows + j];
                inverse[i * rows + j] -= t;
            }
        }
    }
    if f == "det" {
        g.rat(determinant)
    } else if f == "inverse" {
        let entries = inverse
            .into_iter()
            .map(|v| g.rat(v))
            .collect::<Result<_>>()?;
        g.intern(Node::Matrix {
            rows,
            cols,
            entries,
        })
    } else {
        Err(Error::Unsupported(f.into()))
    }
}

pub fn solve(g: &mut Graph, id: Id, var: &str, a: &Assumptions) -> Result<Id> {
    if a.get(var).is_some_and(|a| a.domain == Domain::Complex) {
        return Err(Error::Unsupported(
            "solver currently finds real roots only".into(),
        ));
    }
    let id = algebra::simplify(g, id, a)?;
    let Node::Equation(x, y) = g.nodes[id].clone() else {
        return Err(Error::Unsupported("solve requires an equation".into()));
    };
    let delta = g.intern(Node::Sub(x, y))?;
    let delta = algebra::simplify(g, delta, a)?;
    let p = algebra::polynomial(g, delta, var, 0)?;
    let coefficient = |i| p.get(&i).cloned().unwrap_or_else(Rational::zero);
    let degree = p
        .iter()
        .filter(|(_, v)| !v.is_zero())
        .map(|(n, _)| *n)
        .max()
        .unwrap_or(0);
    let mut values = match degree {
        0 if coefficient(0).is_zero() => {
            return Err(Error::Unsupported(
                "identity: infinitely many solutions; finite root list unavailable".into(),
            ));
        }
        0 => Vec::new(),
        1 => vec![g.rat(-coefficient(0) / coefficient(1))?],
        2 => {
            let (aa, b, c) = (coefficient(2), coefficient(1), coefficient(0));
            let d = &b * &b - Rational::from_integer(4.into()) * &aa * c;
            if d.is_negative() {
                Vec::new()
            } else {
                let d = g.rat(d)?;
                let radical = g.intern(Node::Call("sqrt".into(), d))?;
                let radical = algebra::simplify(g, radical, a)?;
                let b = g.rat(-b)?;
                let denom = g.rat(Rational::from_integer(2.into()) * aa)?;
                let mut out = Vec::new();
                for sign in [false, true] {
                    let top = g.intern(if sign {
                        Node::Add(b, radical)
                    } else {
                        Node::Sub(b, radical)
                    })?;
                    let root = g.intern(Node::Div(top, denom))?;
                    let root = algebra::simplify(g, root, a)?;
                    if !out.contains(&root) {
                        out.push(root);
                    }
                }
                out
            }
        }
        _ => return Err(Error::Unsupported("solve: degree at most two".into())),
    };
    if let Some(a) = a.get(var)
        && (a.nonzero || matches!(a.domain, Domain::Positive | Domain::Nonnegative))
    {
        let mut retained = Vec::new();
        for root in values {
            let v = g.value(root).ok_or_else(|| {
                Error::Unsupported("non-rational root filtering under sign assumptions".into())
            })?;
            if !(a.nonzero && v.is_zero()
                || a.domain == Domain::Positive && !v.is_positive()
                || a.domain == Domain::Nonnegative && v.is_negative())
            {
                retained.push(root);
            }
        }
        values = retained;
    }
    g.intern(Node::Solutions {
        variable: var.into(),
        values,
    })
}

pub fn numeric(
    g: &mut Graph,
    id: Id,
    bindings: &BTreeMap<String, f64>,
    a: &Assumptions,
) -> Result<(Option<Id>, Option<f64>)> {
    if has_indefinite_integral(g, id) {
        return Err(Error::Unsupported(
            "choose an integration constant before numeric evaluation".into(),
        ));
    }
    let id = algebra::simplify(g, id, a)?;
    fn go(g: &mut Graph, id: Id, b: &BTreeMap<String, f64>) -> Result<Id> {
        let node = g.nodes[id].clone();
        match node {
            Node::Matrix { .. } | Node::Equation(..) | Node::Solutions { .. } => {
                let cs = node
                    .children()
                    .into_iter()
                    .map(|c| go(g, c, b))
                    .collect::<Result<Vec<_>>>()?;
                g.intern(rebuild(&node, &cs))
            }
            _ => {
                let v = g.numeric(id, b)?;
                g.rat(Rational::from_float(v).ok_or(Error::Limit)?)
            }
        }
    }
    if matches!(
        g.nodes[id],
        Node::Matrix { .. } | Node::Equation(..) | Node::Solutions { .. }
    ) {
        Ok((Some(go(g, id, bindings)?), None))
    } else {
        Ok((None, Some(g.numeric(id, bindings)?)))
    }
}

pub fn kind(g: &Graph, id: Id) -> &'static str {
    match g.nodes[id] {
        Node::Integral { .. } => "integral",
        Node::Derivative { .. } => "derivative",
        Node::Aggregate { .. } => "aggregate",
        _ => match shape(g, id) {
            Ok(Shape::Matrix(..)) => "matrix",
            Ok(Shape::Relation) => "equation",
            Ok(Shape::Solutions) => "solutions",
            _ => "scalar",
        },
    }
}
pub fn has_indefinite_integral(g: &Graph, root: Id) -> bool {
    let mut todo = vec![root];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(id) = todo.pop() {
        if matches!(g.nodes[id], Node::Integral { lower: None, .. }) {
            return true;
        }
        if seen.insert(id) {
            todo.extend(g.nodes[id].children());
        }
    }
    false
}
