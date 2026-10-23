use std::collections::BTreeMap;
use symbio_core::{
    graph::{Graph, Node},
    *,
};
fn run(source: &str, operation: &str) -> (AppCore, Snapshot) {
    let mut c = AppCore::new(Project::new("structured").unwrap()).unwrap();
    c.apply(Request {
        expected_revision: 0,
        actor: Actor::Human,
        command: Command::DefineLatex {
            id: Some(1),
            name: "f".into(),
            source: source.into(),
        },
    })
    .unwrap();
    let s = c
        .apply(Request {
            expected_revision: 1,
            actor: Actor::Human,
            command: Command::Transform {
                id: 1,
                operation: operation.into(),
                variable: "x".into(),
                bindings: BTreeMap::new(),
            },
        })
        .unwrap();
    (c, s)
}
fn root(s: &Snapshot) -> usize {
    s.cells[0].cell.output.as_ref().unwrap().root.unwrap()
}
#[test]
fn exact_integrals_and_bound_variable_scopes() {
    for (src, answer) in [
        (r"\int_{0}^{1} x^2 \, dx", "1/3"),
        (r"\int_1^0 x^2 dx", "-1/3"),
        (r"\int_{-1}^{1} (x^3+2x^2)\,\mathrm{d}x", "4/3"),
        (r"\int_0^1 \int_0^2 t^2 dt dx", "8/3"),
    ] {
        let (c, s) = run(src, "compute");
        assert_eq!(c.project().graph.text(root(&s)), answer, "{src}");
        assert!(s.cells[0].symbols.is_empty());
    }
    let (c, s) = run(r"\int_0^y x^2 dx", "compute");
    assert_eq!(s.cells[0].symbols, ["y"]);
    assert_eq!(
        c.project()
            .graph
            .numeric(root(&s), &BTreeMap::from([("y".into(), 3.0)]))
            .unwrap(),
        9.0
    );
    let (c, s) = run(r"\int x^2 dx", "compute");
    assert!(
        s.cells[0]
            .cell
            .output
            .as_ref()
            .unwrap()
            .conditions
            .iter()
            .any(|s| s.contains("arbitrary constant"))
    );
    assert_eq!(
        c.project()
            .graph
            .numeric(root(&s), &BTreeMap::from([("x".into(), 3.0)]))
            .unwrap(),
        9.0
    );
    let preview = latex::preview(r"\int_0^x x^2 dx").unwrap();
    assert_eq!(preview.symbols, ["x"]);
    assert!(preview.mathml.contains("<msubsup>"));
}
#[test]
fn matrix_arithmetic_determinant_inverse_and_transpose() {
    let a = r"\begin{bmatrix}1&2\\3&4\end{bmatrix}";
    let (c, s) = run(&format!("{a}{a}"), "compute");
    let Node::Matrix {
        entries,
        rows,
        cols,
    } = &c.project().graph.nodes[root(&s)]
    else {
        panic!()
    };
    assert_eq!((*rows, *cols), (2, 2));
    assert_eq!(
        entries
            .iter()
            .map(|n| c.project().graph.text(*n))
            .collect::<Vec<_>>(),
        ["7", "10", "15", "22"]
    );
    let (c, s) = run(a, "determinant");
    assert_eq!(c.project().graph.text(root(&s)), "-2");
    let (c, s) = run(&format!("{a}^{{-1}}{a}"), "compute");
    let Node::Matrix { entries, .. } = &c.project().graph.nodes[root(&s)] else {
        panic!()
    };
    assert_eq!(
        entries
            .iter()
            .map(|n| c.project().graph.text(*n))
            .collect::<Vec<_>>(),
        ["1", "0", "0", "1"]
    );
    let (c, s) = run(r"\begin{pmatrix}1&2&3\\4&5&6\end{pmatrix}^{T}", "compute");
    assert!(matches!(
        c.project().graph.nodes[root(&s)],
        Node::Matrix {
            rows: 3,
            cols: 2,
            ..
        }
    ));
    assert!(
        s.cells[0]
            .result_mathml
            .as_ref()
            .unwrap()
            .contains("<mtable")
    );
    let (c, s) = run(r"\begin{vmatrix}1&2\\3&4\end{vmatrix}", "compute");
    assert_eq!(c.project().graph.text(root(&s)), "-2");
    let (c, s) = run(&format!("0{a}+{a}"), "compute");
    assert!(matches!(
        c.project().graph.nodes[root(&s)],
        Node::Matrix {
            rows: 2,
            cols: 2,
            ..
        }
    ));
}
#[test]
fn solve_real_equations_and_keep_equation_structure() {
    for (source, expected) in [
        ("2x+3=7", vec!["2"]),
        ("x^{2}-5x+6=0", vec!["2", "3"]),
        ("x^{2}+1=0", vec![]),
        ("x^{2}-2x+1=0", vec!["1"]),
    ] {
        let (c, s) = run(source, "solve");
        assert!(matches!(
            c.project().graph.nodes[s.cells[0].cell.root],
            Node::Equation(..)
        ));
        let Node::Solutions { values, .. } = &c.project().graph.nodes[root(&s)] else {
            panic!()
        };
        assert_eq!(
            values
                .iter()
                .map(|n| c.project().graph.text(*n))
                .collect::<Vec<_>>(),
            expected
        );
    }
    let (c, s) = run("x^2=2", "solve");
    let Node::Solutions { values, .. } = &c.project().graph.nodes[root(&s)] else {
        panic!()
    };
    assert_eq!(values.len(), 2);
    for root in values {
        let v = c.project().graph.numeric(*root, &BTreeMap::new()).unwrap();
        assert!((v * v - 2.0).abs() < 1e-12);
    }
}
#[test]
fn keyboard_calculus_templates_and_symbols() {
    for (source, answer) in [
        (r"\frac{d}{dx}\left(x^3+2x\right)", "((3 * (x ^ 2)) + 2)"),
        (r"\sum_{i=1}^{4}{i^2}", "30"),
        (r"\prod_{i=1}^{4}{i}", "24"),
        (r"\left\lfloor\frac{7}{2}\right\rfloor", "3"),
    ] {
        let (c, s) = run(source, "compute");
        assert_eq!(c.project().graph.text(root(&s)), answer, "{source}");
    }
    let p = latex::preview(r"x_{1}^2+\theta").unwrap();
    assert_eq!(p.symbols, ["theta", "x_1"]);
    assert!(p.mathml.contains("<msub>"));
    assert!(p.mathml.contains('θ'));
}
#[test]
fn malformed_structures_and_shapes_fail_atomically() {
    for source in [
        r"\begin{bmatrix}1&2\\3\end{bmatrix}",
        r"\begin{bmatrix}1\end{pmatrix}",
        r"\int_0 x dx",
        r"\int_0^1 x^2",
        r"\begin{bmatrix}1&2\end{bmatrix}+1",
        r"\begin{bmatrix}1&2\end{bmatrix}\begin{bmatrix}1&2\end{bmatrix}",
        "x=1=2",
    ] {
        assert!(latex::preview(source).is_err(), "{source}");
    }
    let mut c = AppCore::new(Project::new("test").unwrap()).unwrap();
    c.apply(Request {
        expected_revision: 0,
        actor: Actor::Human,
        command: Command::DefineLatex {
            id: Some(1),
            name: "f".into(),
            source: r"\begin{bmatrix}1&2\\2&4\end{bmatrix}".into(),
        },
    })
    .unwrap();
    let before = serde_json::to_value(c.project()).unwrap();
    assert!(
        c.apply(Request {
            expected_revision: 1,
            actor: Actor::Human,
            command: Command::Transform {
                id: 1,
                operation: "inverse".into(),
                variable: "x".into(),
                bindings: BTreeMap::new()
            }
        })
        .is_err()
    );
    assert_eq!(before, serde_json::to_value(c.project()).unwrap());
}
#[test]
fn structured_roundtrip_and_graph_validation() {
    for source in [
        r"\int_0^1 x^2 dx",
        r"\begin{bmatrix}1&2\\3&4\end{bmatrix}",
        "x^2=2",
    ] {
        let (c, _) = run(
            source,
            if source.contains('=') {
                "solve"
            } else {
                "compute"
            },
        );
        let json = serde_json::to_string(c.project()).unwrap();
        let d = AppCore::new(serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(d.snapshot().cells[0].cell.source, source);
    }
    let mut g = Graph::default();
    let n = g.number(1).unwrap();
    g.intern(Node::Matrix {
        rows: usize::MAX,
        cols: 2,
        entries: vec![n],
    })
    .unwrap();
    assert!(g.rebuild().is_err());
}

#[test]
fn binding_capture_domains_and_numeric_trust() {
    let (c, s) = run(r"\int_0^1 \int_0^x t dt dx", "compute");
    assert_eq!(c.project().graph.text(root(&s)), "1/6");
    let (c, s) = run(r"\int_0^{\pi}\sin(x)dx", "compute");
    assert_eq!(c.project().graph.text(root(&s)), "2");
    let mut g = Graph::default();
    let integral = latex::parse(&mut g, r"\int_0^1 y dx").unwrap();
    let x = parser::parse(&mut g, "x").unwrap();
    assert!(structured::substitute(&mut g, integral, "y", x).is_err());
    let integral = latex::parse(&mut g, r"\int_0^1 \sqrt{x^2} dx").unwrap();
    let positive = BTreeMap::from([(
        "x".into(),
        Assumption {
            domain: Domain::Positive,
            nonzero: true,
            dimension: None,
        },
    )]);
    assert!(algebra::simplify(&mut g, integral, &positive).is_err());
    let integral = latex::parse(&mut g, r"\int x^2 dx").unwrap();
    assert!(
        structured::numeric(
            &mut g,
            integral,
            &BTreeMap::from([("x".into(), 2.0)]),
            &BTreeMap::new()
        )
        .is_err()
    );
    let (c, s) = run(r"\begin{bmatrix}\sqrt{2}&\sin(1)\end{bmatrix}", "evaluate");
    let o = s.cells[0].cell.output.as_ref().unwrap();
    assert_eq!(o.trust, Trust::Approximate);
    assert!(
        c.project()
            .graph
            .approximate_mathml(root(&s))
            .contains("1.414213")
    );
    assert!(
        !s.cells[0]
            .result_mathml
            .as_ref()
            .unwrap()
            .contains("<mfrac>")
    );
}
#[test]
fn switching_formats_preserves_scalar_semantics() {
    for src in ["sqrt(x^2)", "sin(x)+0.5", "velocity*x", "x^T"] {
        let tex = latex::convert(src, SourceFormat::Expression, SourceFormat::Latex).unwrap();
        let plain = latex::convert(&tex, SourceFormat::Latex, SourceFormat::Expression).unwrap();
        let mut g = Graph::default();
        let a = parser::parse(&mut g, src).unwrap();
        let b = parser::parse(&mut g, &plain).unwrap();
        let bindings = BTreeMap::from([
            ("x".into(), 2.0),
            ("velocity".into(), 3.0),
            ("T".into(), 2.0),
        ]);
        assert_eq!(
            g.numeric(a, &bindings).unwrap(),
            g.numeric(b, &bindings).unwrap(),
            "{src}"
        );
    }
    assert!(
        latex::convert(
            r"\int_0^1 x dx",
            SourceFormat::Latex,
            SourceFormat::Expression
        )
        .is_err()
    );
}
