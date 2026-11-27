//! Exact matrix arithmetic over shared SEG nodes.
use super::{rebuild, shape};
use crate::{
    Error, Result,
    algebra::{self, Assumptions},
    graph::{Graph, Id, Node, Rational},
};
use num_traits::{One, ToPrimitive, Zero};

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

pub(crate) fn matrix_dimension(g: &Graph, order: Id) -> Result<usize> {
    g.value(order)
        .filter(|value| value.is_integer())
        .and_then(|value| value.to_usize())
        .filter(|order| (1..=8).contains(order))
        .ok_or_else(|| Error::Domain("matrix dimension must be an integer from 1 to 8".into()))
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
    match f {
        "rowcount" => return g.number(rows as i64),
        "columncount" => return g.number(cols as i64),
        _ => {}
    }
    if f != "rank" && rows != cols {
        return Err(Error::Domain("square matrix required".into()));
    }
    if f == "trace" {
        let mut trace = entries[0];
        for i in 1..rows {
            trace = g.intern(Node::Add(trace, entries[i * cols + i]))?;
        }
        return algebra::simplify(g, trace, a);
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
    if f == "rank" {
        let mut rank = 0;
        for col in 0..cols {
            let Some(pivot) = (rank..rows).find(|row| !values[row * cols + col].is_zero()) else {
                continue;
            };
            for j in col..cols {
                values.swap(rank * cols + j, pivot * cols + j);
            }
            for row in rank + 1..rows {
                let factor = &values[row * cols + col] / &values[rank * cols + col];
                for j in col..cols {
                    let term = &factor * &values[rank * cols + j];
                    values[row * cols + j] -= term;
                }
            }
            rank += 1;
        }
        return g.number(rank as i64);
    }
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
