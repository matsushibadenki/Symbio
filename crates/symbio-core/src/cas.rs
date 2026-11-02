//! Optional local CAS adapter. Only bounded rational coefficient vectors cross the process boundary.
use crate::{
    algebra::{self, Polynomial},
    graph::{Graph, Id, Node, Rational},
    *,
};
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command as Process, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};
#[derive(Clone)]
enum Runtime {
    Environment,
    Disabled,
    Python(PathBuf, Option<String>),
}
static RUNTIME: Mutex<Runtime> = Mutex::new(Runtime::Environment);
#[derive(Clone, Serialize)]
pub struct State {
    pub python: Option<PathBuf>,
    pub version: Option<String>,
}
pub fn state() -> State {
    match &*RUNTIME.lock().expect("CAS settings") {
        Runtime::Environment => State {
            python: std::env::var_os("SYMBIO_CAS_PYTHON").map(PathBuf::from),
            version: None,
        },
        Runtime::Disabled => State {
            python: None,
            version: None,
        },
        Runtime::Python(p, v) => State {
            python: Some(p.clone()),
            version: v.clone(),
        },
    }
}
pub fn configure(python: Option<PathBuf>, version: Option<String>) {
    *RUNTIME.lock().expect("CAS settings") = python
        .map(|p| Runtime::Python(p, version))
        .unwrap_or(Runtime::Disabled);
}
#[derive(Serialize)]
struct Input<'a> {
    protocol: u8,
    method: &'a str,
    coefficients: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    protocol: u8,
    version: String,
    content: String,
    factors: Vec<Factor>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Factor {
    coefficients: Vec<String>,
    exponent: u32,
}
fn run(python: &Path, input: &Input<'_>, timeout: Duration) -> Result<Response> {
    if !python.is_absolute() || !python.is_file() {
        return Err(Error::CasUnavailable);
    }
    let mut child = Process::new(python)
        .args(["-I", "-B", "-c", include_str!("cas_worker.py")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| Error::CasUnavailable)?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .by_ref()
            .take(262145)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr
            .by_ref()
            .take(4097)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let write = child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(input)?);
    let start = Instant::now();
    let mut expired = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                expired = true;
                let _ = child.kill();
                break child.wait();
            }
        }
    };
    let bytes = out
        .join()
        .map_err(|_| Error::CasFailure)?
        .map_err(|_| Error::CasFailure)?;
    let _ = err.join();
    if expired {
        return Err(Error::CasTimeout);
    }
    let status = status.map_err(|_| Error::CasFailure)?;
    if status.code() == Some(42) {
        return Err(Error::CasUnavailable);
    }
    if !status.success() || write.is_err() {
        return Err(Error::CasFailure);
    }
    if bytes.len() > 262144 {
        return Err(Error::CasInvalid);
    }
    let response: Response = serde_json::from_slice(&bytes).map_err(|_| Error::CasInvalid)?;
    if response.protocol != 1
        || response.version.is_empty()
        || response.version.len() > 64
        || !response
            .version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b))
    {
        return Err(Error::CasInvalid);
    }
    Ok(response)
}
pub fn probe(python: &Path) -> Result<String> {
    Ok(run(
        python,
        &Input {
            protocol: 1,
            method: "probe",
            coefficients: vec![],
        },
        Duration::from_secs(8),
    )?
    .version)
}
fn bounded(value: &str) -> Result<Rational> {
    let v = graph::rational(value).map_err(|_| Error::CasInvalid)?;
    if v.numer().bits() > 256 || v.denom().bits() > 256 {
        return Err(Error::Limit);
    }
    Ok(v)
}
fn normalize(mut p: Polynomial) -> Polynomial {
    p.retain(|_, c| !c.is_zero());
    p
}
fn validate_polynomial(p: &Polynomial) -> Result<()> {
    if p.keys().any(|d| *d > 32)
        || p.values()
            .any(|c| c.numer().bits() > 256 || c.denom().bits() > 256)
    {
        return Err(Error::Limit);
    }
    Ok(())
}
fn accept(
    graph: &mut Graph,
    variable: &str,
    input: &Polynomial,
    response: Response,
) -> Result<(Id, String)> {
    if response.factors.len() > 32 {
        return Err(Error::CasInvalid);
    }
    let content = bounded(&response.content)?;
    let mut root = graph.rat(content)?;
    let mut degree = 0u32;
    for factor in response.factors {
        if !(2..=33).contains(&factor.coefficients.len())
            || factor.exponent == 0
            || factor.exponent > 32
        {
            return Err(Error::CasInvalid);
        }
        degree = degree
            .checked_add((factor.coefficients.len() as u32 - 1) * factor.exponent)
            .ok_or(Error::CasInvalid)?;
        if degree > 32 {
            return Err(Error::CasInvalid);
        }
        let coefficients = factor
            .coefficients
            .iter()
            .map(|s| bounded(s))
            .collect::<Result<Vec<_>>>()?;
        if coefficients.last().is_none_or(Zero::is_zero) {
            return Err(Error::CasInvalid);
        }
        let p = coefficients
            .into_iter()
            .enumerate()
            .map(|(i, c)| (i as u32, c))
            .collect();
        let mut f = algebra::from_polynomial(graph, &p, variable)?;
        if factor.exponent > 1 {
            let exponent = graph.number(factor.exponent as i64)?;
            f = graph.intern(Node::Pow(f, exponent))?;
        }
        root = if graph.value(root).is_some_and(|c| c.is_one()) {
            f
        } else {
            graph.intern(Node::Mul(root, f))?
        };
    }
    let expanded = normalize(algebra::polynomial(graph, root, variable, 0)?);
    if expanded != normalize(input.clone()) {
        return Err(Error::CasInvalid);
    }
    Ok((root, response.version))
}
fn factor_response(p: &Polynomial) -> Result<Response> {
    validate_polynomial(p)?;
    let python = state().python.ok_or(Error::CasUnavailable)?;
    let degree = p.keys().next_back().copied().unwrap_or(0);
    let coefficients = (0..=degree)
        .map(|d| graph::rat_text(p.get(&d).unwrap_or(&Rational::zero())))
        .collect();
    run(
        &python,
        &Input {
            protocol: 1,
            method: "factor",
            coefficients,
        },
        Duration::from_secs(8),
    )
}
pub fn factor(graph: &mut Graph, root: Id, variable: &str) -> Result<(Id, String)> {
    // Never cancel variable denominators before checking the polynomial scope.
    let p = normalize(algebra::polynomial(graph, root, variable, 0)?);
    let response = factor_response(&p)?;
    accept(graph, variable, &p, response)
}
fn solve_factors(
    graph: &mut Graph,
    variable: &str,
    p: &Polynomial,
    response: Response,
    assumptions: &algebra::Assumptions,
) -> Result<(Id, String)> {
    if p.is_empty() {
        return Err(Error::Unsupported(
            "identity: finite root list unavailable".into(),
        ));
    }
    // Completeness follows from an exact product identity and solving every factor.
    let (_, version) = accept(graph, variable, p, response.clone())?;
    if response.factors.iter().any(|f| f.coefficients.len() > 3) {
        return Err(Error::Unsupported(
            "CAS solve requires only linear or quadratic rational factors".into(),
        ));
    }
    let zero = graph.number(0)?;
    let mut values = Vec::new();
    for factor in response.factors {
        let coefficients = factor
            .coefficients
            .iter()
            .map(|v| bounded(v))
            .collect::<Result<Vec<_>>>()?;
        let leading = coefficients.last().ok_or(Error::CasInvalid)?.clone();
        // Monic normalization gives proportional factors the same root expressions.
        let polynomial = coefficients
            .into_iter()
            .enumerate()
            .map(|(i, c)| (i as u32, c / &leading))
            .collect();
        let root = algebra::from_polynomial(graph, &polynomial, variable)?;
        let equation = graph.intern(Node::Equation(root, zero))?;
        let solutions = structured::solve(graph, equation, variable, assumptions)?;
        let Node::Solutions { values: roots, .. } = graph.nodes[solutions].clone() else {
            return Err(Error::CasInvalid);
        };
        for root in roots {
            if !values.contains(&root) {
                values.push(root);
            }
        }
    }
    if values.iter().all(|id| graph.value(*id).is_some()) {
        values.sort_by_key(|id| graph.value(*id).expect("rational root"));
    }
    let result = graph.intern(Node::Solutions {
        variable: variable.into(),
        values,
    })?;
    Ok((result, version))
}
/// Exact real solutions when the rational factorization has no factor above degree two.
pub fn solve(
    graph: &mut Graph,
    root: Id,
    variable: &str,
    assumptions: &algebra::Assumptions,
) -> Result<(Id, String)> {
    if assumptions
        .get(variable)
        .is_some_and(|a| a.domain == Domain::Complex)
    {
        return Err(Error::Unsupported(
            "CAS solver finds real roots only".into(),
        ));
    }
    let Node::Equation(left, right) = graph.nodes[root].clone() else {
        return Err(Error::Unsupported("CAS solve requires an equation".into()));
    };
    let delta = graph.intern(Node::Sub(left, right))?;
    let polynomial = normalize(algebra::polynomial(graph, delta, variable, 0)?);
    if polynomial.is_empty() {
        return Err(Error::Unsupported(
            "identity: finite root list unavailable".into(),
        ));
    }
    let response = factor_response(&polynomial)?;
    solve_factors(graph, variable, &polynomial, response, assumptions)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response(content: &str, factors: Vec<Factor>) -> Response {
        Response {
            protocol: 1,
            version: "1.14.0".into(),
            content: content.into(),
            factors,
        }
    }
    fn f(c: &[&str], exponent: u32) -> Factor {
        Factor {
            coefficients: c.iter().map(|s| s.to_string()).collect(),
            exponent,
        }
    }
    #[test]
    fn valid_factorization_is_checked_by_exact_rust_expansion() {
        let mut g = Graph::default();
        let root = parser::parse(&mut g, "x^2-1").unwrap();
        let p = algebra::polynomial(&g, root, "x", 0).unwrap();
        let (r, _) = accept(
            &mut g,
            "x",
            &p,
            response("1", vec![f(&["-1", "1"], 1), f(&["1", "1"], 1)]),
        )
        .unwrap();
        assert!(matches!(g.nodes[r], Node::Mul(..)));
        assert_eq!(
            normalize(algebra::polynomial(&g, r, "x", 0).unwrap()),
            normalize(p)
        );
    }
    #[test]
    fn fabricated_factors_and_malformed_output_are_rejected() {
        let mut g = Graph::default();
        let root = parser::parse(&mut g, "x^2-1").unwrap();
        let p = algebra::polynomial(&g, root, "x", 0).unwrap();
        for r in [
            response("1", vec![f(&["2", "1"], 2)]),
            response("1", vec![f(&["1", "0"], 2)]),
            response("1", vec![f(&["1", "1"], 33)]),
            response("1", vec![f(&["1"], 1)]),
            response("0", vec![]),
        ] {
            assert!(matches!(accept(&mut g, "x", &p, r), Err(Error::CasInvalid)));
        }
    }
    #[test]
    fn rational_content_repeated_factors_and_zero_are_exact() {
        let mut g = Graph::default();
        let root = parser::parse(&mut g, "(x+1)^3/2").unwrap();
        let p = algebra::polynomial(&g, root, "x", 0).unwrap();
        assert!(accept(&mut g, "x", &p, response("1/2", vec![f(&["1", "1"], 3)])).is_ok());
        assert!(accept(&mut g, "x", &Polynomial::new(), response("0", vec![])).is_ok());
    }
    #[test]
    fn polynomial_scope_and_response_limits_are_explicit() {
        let mut g = Graph::default();
        for source in ["x/x", "sin(x)", "x+y"] {
            let r = parser::parse(&mut g, source).unwrap();
            assert!(algebra::polynomial(&g, r, "x", 0).is_err());
        }
        assert!(bounded(&"9".repeat(100)).is_err());
        assert!(
            serde_json::from_str::<Response>(
                r#"{"protocol":1,"version":"1.0","content":"1","factors":[],"code":"run"}"#
            )
            .is_err()
        );
        assert!(probe(Path::new("python3")).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn worker_timeout_and_malformed_transport_are_rejected() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let worker = dir.path().join("worker");
        let input = Input {
            protocol: 1,
            method: "probe",
            coefficients: vec![],
        };
        std::fs::write(&worker, "#!/bin/sh\nexec /bin/sleep 1\n").unwrap();
        std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(matches!(
            run(&worker, &input, Duration::from_millis(40)),
            Err(Error::CasTimeout)
        ));
        std::fs::write(&worker, "#!/bin/sh\nprintf '{}'\n").unwrap();
        assert!(matches!(
            run(&worker, &input, Duration::from_secs(1)),
            Err(Error::CasInvalid)
        ));
    }
    #[test]
    fn solver_checks_all_factors_and_returns_exact_real_roots() {
        let mut g = Graph::default();
        let r = parser::parse(&mut g, "x^4-5*x^2+6").unwrap();
        let p = algebra::polynomial(&g, r, "x", 0).unwrap();
        let (r, _) = solve_factors(
            &mut g,
            "x",
            &p,
            response("1", vec![f(&["-2", "0", "1"], 1), f(&["-3", "0", "1"], 1)]),
            &algebra::Assumptions::new(),
        )
        .unwrap();
        let Node::Solutions { values, .. } = &g.nodes[r] else {
            panic!()
        };
        assert_eq!(values.len(), 4);
        assert!(values.iter().all(|r| g.value(*r).is_none()));
        let r = parser::parse(&mut g, "(2*x-4)*(x-2)").unwrap();
        let p = algebra::polynomial(&g, r, "x", 0).unwrap();
        let (r, _) = solve_factors(
            &mut g,
            "x",
            &p,
            response("1", vec![f(&["-4", "2"], 1), f(&["-2", "1"], 1)]),
            &algebra::Assumptions::new(),
        )
        .unwrap();
        let Node::Solutions { values, .. } = &g.nodes[r] else {
            panic!()
        };
        assert_eq!(values.len(), 1);
        assert_eq!(g.value(values[0]).unwrap(), graph::rational("2").unwrap());
    }
    #[test]
    fn solver_rejects_partial_lists_and_incorrect_factorizations() {
        let mut g = Graph::default();
        let r = parser::parse(&mut g, "(x-1)*(x^3-x-1)").unwrap();
        let p = algebra::polynomial(&g, r, "x", 0).unwrap();
        assert!(matches!(
            solve_factors(
                &mut g,
                "x",
                &p,
                response("1", vec![f(&["-1", "1"], 1), f(&["-1", "-1", "0", "1"], 1)]),
                &algebra::Assumptions::new()
            ),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            solve_factors(
                &mut g,
                "x",
                &p,
                response("1", vec![f(&["-1", "1"], 1)]),
                &algebra::Assumptions::new()
            ),
            Err(Error::CasInvalid)
        ));
        assert!(
            solve_factors(
                &mut g,
                "x",
                &Polynomial::new(),
                response("0", vec![]),
                &algebra::Assumptions::new()
            )
            .is_err()
        );
        let r = parser::parse(&mut g, "x^2-2").unwrap();
        let p = algebra::polynomial(&g, r, "x", 0).unwrap();
        let assumptions = algebra::Assumptions::from([(
            "x".into(),
            Assumption {
                domain: Domain::Positive,
                ..Default::default()
            },
        )]);
        assert!(matches!(
            solve_factors(
                &mut g,
                "x",
                &p,
                response("1", vec![f(&["-2", "0", "1"], 1)]),
                &assumptions
            ),
            Err(Error::Unsupported(_))
        ));
    }
    #[test]
    fn solver_handles_no_real_roots_constants_and_sign_assumptions() {
        let mut g = Graph::default();
        let r = parser::parse(&mut g, "(x^2+1)^2").unwrap();
        let p = algebra::polynomial(&g, r, "x", 0).unwrap();
        let (r, _) = solve_factors(
            &mut g,
            "x",
            &p,
            response("1", vec![f(&["1", "0", "1"], 2)]),
            &algebra::Assumptions::new(),
        )
        .unwrap();
        assert!(matches!(&g.nodes[r],Node::Solutions{values,..} if values.is_empty()));
        let r = parser::parse(&mut g, "7").unwrap();
        let p = algebra::polynomial(&g, r, "x", 0).unwrap();
        let (r, _) = solve_factors(
            &mut g,
            "x",
            &p,
            response("7", vec![]),
            &algebra::Assumptions::new(),
        )
        .unwrap();
        assert!(matches!(&g.nodes[r],Node::Solutions{values,..} if values.is_empty()));
        let r = parser::parse(&mut g, "x^4-x^2").unwrap();
        let p = algebra::polynomial(&g, r, "x", 0).unwrap();
        let assumptions = algebra::Assumptions::from([(
            "x".into(),
            Assumption {
                domain: Domain::Positive,
                ..Default::default()
            },
        )]);
        let (r, _) = solve_factors(
            &mut g,
            "x",
            &p,
            response(
                "1",
                vec![f(&["0", "1"], 2), f(&["-1", "1"], 1), f(&["1", "1"], 1)],
            ),
            &assumptions,
        )
        .unwrap();
        assert!(
            matches!(&g.nodes[r],Node::Solutions{values,..} if values.len()==1 && g.value(values[0])==Some(graph::rational("1").unwrap()))
        );
    }
    #[test]
    fn solver_includes_zero_root() {
        let mut g = Graph::default();
        let r = parser::parse(&mut g, "x^3-x").unwrap();
        let p = algebra::polynomial(&g, r, "x", 0).unwrap();
        let (r, _) = solve_factors(
            &mut g,
            "x",
            &p,
            response(
                "1",
                vec![f(&["-1", "1"], 1), f(&["0", "1"], 1), f(&["1", "1"], 1)],
            ),
            &algebra::Assumptions::new(),
        )
        .unwrap();
        assert!(matches!(&g.nodes[r],Node::Solutions{values,..} if values.len()==3));
        g.rebuild().unwrap();
    }
    #[test]
    fn persisted_solution_sets_have_a_bounded_degree_sized_capacity() {
        let mut g = Graph::default();
        let values = (0..graph::MAX_SOLUTIONS)
            .map(|i| g.number(i as i64).unwrap())
            .collect::<Vec<_>>();
        g.intern(Node::Solutions {
            variable: "x".into(),
            values: values.clone(),
        })
        .unwrap();
        g.rebuild().unwrap();
        let mut too_many = values;
        too_many.push(g.number(32).unwrap());
        g.intern(Node::Solutions {
            variable: "x".into(),
            values: too_many,
        })
        .unwrap();
        assert!(matches!(g.rebuild(), Err(Error::Limit)));
    }
}
