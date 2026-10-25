//! Explicit exact rational substitution, separate from symbolic reduction and float64 evaluation.
use crate::algebra::Assumptions;
use crate::{
    Error, Result, algebra,
    graph::{Graph, Id, Node, Rational},
    model::Domain,
    parser, structured,
};
use num_traits::{Signed, Zero};
use std::collections::BTreeMap;

pub fn literal(source: &str) -> Result<Rational> {
    if source.len() > 256 {
        return Err(Error::Limit);
    }
    if source
        .chars()
        .any(|c| !c.is_ascii_digit() && !c.is_ascii_whitespace() && !".+-/".contains(c))
    {
        return Err(Error::Unsupported(
            "substitution values must be rational literals".into(),
        ));
    }
    let mut graph = Graph::default();
    let root = parser::parse(&mut graph, source)?;
    // Deliberately bounded to integer/decimal/fraction literals with an optional sign.
    if graph
        .nodes
        .iter()
        .any(|n| !matches!(n, Node::Number(_) | Node::Mul(..) | Node::Div(..)))
    {
        return Err(Error::Unsupported(
            "substitution values must be rational literals".into(),
        ));
    }
    let root = algebra::simplify(&mut graph, root, &Assumptions::new())?;
    graph
        .value(root)
        .ok_or_else(|| Error::Unsupported("substitution values must be rational literals".into()))
}
pub fn validate(values: &BTreeMap<String, String>) -> Result<()> {
    if values.is_empty() || values.len() > 64 {
        return Err(Error::Invalid("substitution values".into()));
    }
    for (name, source) in values {
        if !crate::graph::valid_symbol(name) {
            return Err(Error::Invalid("substitution variable".into()));
        }
        literal(source)?;
    }
    Ok(())
}
pub fn apply(
    graph: &mut Graph,
    root: Id,
    values: &BTreeMap<String, String>,
    assumptions: &Assumptions,
) -> Result<Id> {
    validate(values)?;
    let mut pending = vec![root];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Node::Derivative { variable, .. }
        | Node::Integral {
            variable,
            lower: None,
            ..
        } = &graph.nodes[id]
            && values.contains_key(variable)
        {
            return Err(Error::Unsupported(
                "evaluate the calculus operator before substituting its variable".into(),
            ));
        }
        pending.extend(graph.nodes[id].children());
    }
    let symbols = graph.symbols(root);
    let mut result = root;
    for (name, source) in values {
        if !symbols.contains(name) {
            return Err(Error::Invalid(
                "substitution variable is not free in the expression".into(),
            ));
        }
        let value = literal(source)?;
        if let Some(a) = assumptions.get(name) {
            if a.dimension.is_some_and(|d| d != [0, 0, 0]) {
                return Err(Error::Unsupported(
                    "dimensioned substitution requires quantity values".into(),
                ));
            }
            if (a.nonzero && value.is_zero())
                || (a.domain == Domain::Positive && !value.is_positive())
                || (a.domain == Domain::Nonnegative && value.is_negative())
            {
                return Err(Error::Domain("substitution violates assumption".into()));
            }
        }
        let value = graph.rat(value)?;
        result = structured::substitute(graph, result, name, value)?;
    }
    // Substitute into the authored expression before simplification: singular values
    // cannot be hidden by a prior cancellation such as x/x -> 1.
    algebra::simplify(graph, result, assumptions)
}
