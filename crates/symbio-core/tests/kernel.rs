use std::collections::BTreeMap;
use symbio_core::{
    graph::{Graph, Node},
    model::{Assumption, Domain},
    *,
};
fn core() -> AppCore {
    AppCore::new(Project::new("Test").unwrap()).unwrap()
}
fn apply(c: &mut AppCore, command: Command) -> Snapshot {
    c.apply(Request {
        expected_revision: c.project().revision,
        actor: Actor::Human,
        command,
    })
    .unwrap()
}
fn transform(c: &mut AppCore, op: &str, id: u64) -> Snapshot {
    apply(
        c,
        Command::Transform {
            id,
            operation: op.into(),
            variable: "x".into(),
            bindings: BTreeMap::new(),
        },
    )
}
fn define(c: &mut AppCore, source: &str) -> u64 {
    let s = apply(
        c,
        Command::Define {
            id: None,
            name: format!("e{}", c.project().next_cell),
            source: source.into(),
        },
    );
    s.selected.unwrap()
}
#[test]
fn shared_subexpressions_are_structural() {
    let mut g = Graph::default();
    let root = parser::parse(&mut g, "(x+1)*(x+1)").unwrap();
    let Node::Mul(a, b) = g.nodes[root] else {
        panic!()
    };
    assert_eq!(a, b);
    assert_eq!(g.nodes.len(), 4);
    let other = parser::parse(&mut g, "(1+x)*(x+1)").unwrap();
    let Node::Mul(a, b) = g.nodes[other] else {
        panic!()
    };
    assert_ne!(a, b);
}
#[test]
fn exact_rational_and_precedence() {
    let mut c = core();
    for (source, expected) in [
        ("0.1+0.2", "3/10"),
        ("1/3+1/6", "1/2"),
        ("-2^2", "-4"),
        ("2^3^2", "512"),
        (
            "999999999999999999999999999999+1",
            "1000000000000000000000000000000",
        ),
    ] {
        let id = define(&mut c, source);
        let s = transform(&mut c, "simplify", id);
        assert_eq!(
            s.cells.last().unwrap().result_text.as_deref(),
            Some(expected)
        );
    }
}
#[test]
fn polynomial_derivative_and_integral() {
    let mut c = core();
    let s = transform(&mut c, "differentiate", 1);
    let root = s.cells[0].cell.output.as_ref().unwrap().root.unwrap();
    for x in [-2., 0., 2., 3.] {
        assert_eq!(
            c.project()
                .graph
                .numeric(root, &BTreeMap::from([("x".into(), x)]))
                .unwrap(),
            3. * x * x + 4. * x - 5.
        );
    }
    let id = define(&mut c, "3*x^2 + 4*x - 5");
    let s = transform(&mut c, "integrate", id);
    assert!(
        s.cells
            .last()
            .unwrap()
            .cell
            .output
            .as_ref()
            .unwrap()
            .conditions
            .iter()
            .any(|s| s.contains("constant C"))
    );
}
#[test]
fn domain_preserving_rewrites() {
    let mut c = core();
    let id = define(&mut c, "x/x");
    let s = transform(&mut c, "simplify", id);
    assert_ne!(s.cells.last().unwrap().result_text.as_deref(), Some("1"));
    apply(
        &mut c,
        Command::Assume {
            symbol: "x".into(),
            assumption: Assumption {
                domain: Domain::Real,
                nonzero: true,
                dimension: None,
            },
        },
    );
    assert!(c.snapshot().cells.iter().all(|c| c.cell.output.is_none()));
    let s = transform(&mut c, "simplify", id);
    assert_eq!(s.cells.last().unwrap().result_text.as_deref(), Some("1"));
    for (domain, expected) in [
        (Domain::Real, "abs(x)"),
        (Domain::Nonnegative, "x"),
        (Domain::Complex, "sqrt((x ^ 2))"),
    ] {
        apply(
            &mut c,
            Command::Assume {
                symbol: "x".into(),
                assumption: Assumption {
                    domain,
                    nonzero: false,
                    dimension: None,
                },
            },
        );
        let s = transform(&mut c, "simplify", 2);
        assert_eq!(s.cells[1].result_text.as_deref(), Some(expected));
    }
}
#[test]
fn undefined_domains_are_not_erased() {
    let mut c = core();
    for source in ["0*(1/x)", "(1/x)-(1/x)", "x^0"] {
        let id = define(&mut c, source);
        let s = transform(&mut c, "simplify", id);
        let r = s
            .cells
            .last()
            .unwrap()
            .cell
            .output
            .as_ref()
            .unwrap()
            .root
            .unwrap();
        assert!(
            c.project()
                .graph
                .numeric(r, &BTreeMap::from([("x".into(), 0.)]))
                .is_err()
        );
    }
    for source in ["1/0", "0^0", "0^-2"] {
        let id = define(&mut c, source);
        let before = c.project().revision;
        assert!(
            c.apply(Request {
                expected_revision: before,
                actor: Actor::Human,
                command: Command::Transform {
                    id,
                    operation: "simplify".into(),
                    variable: "x".into(),
                    bindings: BTreeMap::new()
                }
            })
            .is_err()
        );
        assert_eq!(c.project().revision, before);
    }
}
#[test]
fn revisions_fail_atomically_and_undo_keeps_audit() {
    let mut c = core();
    let before = serde_json::to_vec(c.project()).unwrap();
    assert!(
        c.apply(Request {
            expected_revision: 99,
            actor: Actor::Ai,
            command: Command::Branch {
                name: "proposal".into()
            }
        })
        .is_err()
    );
    assert_eq!(serde_json::to_vec(c.project()).unwrap(), before);
    assert!(
        c.apply(Request {
            expected_revision: 0,
            actor: Actor::Ai,
            command: Command::Define {
                id: None,
                name: "bad".into(),
                source: "x + )".into()
            }
        })
        .is_err()
    );
    assert_eq!(serde_json::to_vec(c.project()).unwrap(), before);
    apply(
        &mut c,
        Command::Branch {
            name: "proposal".into(),
        },
    );
    let rev = c.project().revision;
    apply(&mut c, Command::Undo);
    assert_eq!(c.project().active, "main");
    assert!(c.project().revision > rev);
    assert_eq!(c.project().history.len(), 2);
    apply(&mut c, Command::Redo);
    assert_eq!(c.project().active, "proposal");
}
#[test]
fn branches_isolate_assumptions() {
    let mut c = core();
    apply(
        &mut c,
        Command::Branch {
            name: "positive".into(),
        },
    );
    apply(
        &mut c,
        Command::Assume {
            symbol: "x".into(),
            assumption: Assumption {
                domain: Domain::Positive,
                ..Default::default()
            },
        },
    );
    transform(&mut c, "simplify", 2);
    apply(
        &mut c,
        Command::Checkout {
            name: "main".into(),
        },
    );
    assert_eq!(c.project().current().assumptions["x"].domain, Domain::Real);
    assert_eq!(c.snapshot().cells[1].result_text.as_deref(), Some("abs(x)"));
}
#[test]
fn numeric_result_is_not_a_proof() {
    let mut c = core();
    let s = apply(
        &mut c,
        Command::Transform {
            id: 1,
            operation: "evaluate".into(),
            variable: "x".into(),
            bindings: BTreeMap::from([("x".into(), 2.)]),
        },
    );
    let o = s.cells[0].cell.output.as_ref().unwrap();
    assert_eq!(o.value, Some(6.));
    assert_eq!(o.trust, Trust::Approximate);
    assert!(o.precision.contains("no certified"));
}
#[test]
fn dimensional_mismatch_rolls_back() {
    let mut c = core();
    apply(
        &mut c,
        Command::Assume {
            symbol: "length".into(),
            assumption: Assumption {
                dimension: Some([0, 1, 0]),
                ..Default::default()
            },
        },
    );
    apply(
        &mut c,
        Command::Assume {
            symbol: "time".into(),
            assumption: Assumption {
                dimension: Some([0, 0, 1]),
                ..Default::default()
            },
        },
    );
    let before = c.project().revision;
    assert!(
        c.apply(Request {
            expected_revision: before,
            actor: Actor::Human,
            command: Command::Define {
                id: None,
                name: "bad".into(),
                source: "length+time".into()
            }
        })
        .is_err()
    );
    assert_eq!(c.project().revision, before);
    let id = define(&mut c, "length/time");
    assert_eq!(
        c.snapshot()
            .cells
            .iter()
            .find(|c| c.cell.id == id)
            .unwrap()
            .dimension,
        Some([0, 1, -1])
    );
}
#[test]
fn bounded_parser_and_untrusted_graph() {
    let mut g = Graph::default();
    assert!(parser::parse(&mut g, &format!("{}x{}", "(".repeat(100), ")".repeat(100))).is_err());
    assert!(parser::parse(&mut g, "<script>").is_err());
    assert!(parser::parse(&mut g, "run(x)").is_err());
    let mut p = Project::new("Test").unwrap();
    p.graph.nodes.push(Node::Add(999999, 0));
    assert!(p.validate().is_err());
}
#[test]
fn save_roundtrip_lock_external_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.symbio");
    let mut store = ProjectStore::open(&path).unwrap();
    let mut c = core();
    apply(
        &mut c,
        Command::Branch {
            name: "experiment".into(),
        },
    );
    store.save(c.project()).unwrap();
    assert!(ProjectStore::open(&path).is_err());
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.active, "experiment");
    let restored = AppCore::new(loaded).unwrap();
    assert_eq!(restored.snapshot().cells.len(), 2);
    std::fs::write(&path, b"external change").unwrap();
    assert!(matches!(
        store.save(c.project()),
        Err(Error::ExternalChange)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"external change");
}

#[test]
fn untrusted_dimension_extremes_are_rejected_without_overflow() {
    for exponent in [i32::MAX, i32::MIN] {
        let mut project = Project::new("Invalid dimensions").unwrap();
        project
            .branches
            .get_mut("main")
            .unwrap()
            .assumptions
            .get_mut("x")
            .unwrap()
            .dimension = Some([exponent, 0, 0]);
        assert!(AppCore::new(project).is_err());
    }
}
