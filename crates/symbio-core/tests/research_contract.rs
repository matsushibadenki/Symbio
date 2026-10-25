use std::collections::BTreeMap;
use symbio_core::{
    graph::{Node, Rational},
    *,
};
fn core() -> AppCore {
    AppCore::new(Project::empty("Research contract").unwrap()).unwrap()
}
fn edit(c: &mut AppCore, command: Command) -> Snapshot {
    c.apply(Request {
        expected_revision: c.project().revision,
        actor: Actor::Human,
        command,
    })
    .unwrap()
}
fn define(c: &mut AppCore, name: &str, source: &str, latex: bool) -> u64 {
    let command = if latex {
        Command::DefineLatex {
            id: None,
            name: name.into(),
            source: source.into(),
        }
    } else {
        Command::Define {
            id: None,
            name: name.into(),
            source: source.into(),
        }
    };
    edit(c, command).selected.unwrap()
}
fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
fn substitute(c: &mut AppCore, id: u64, pairs: &[(&str, &str)]) -> Snapshot {
    edit(
        c,
        Command::Substitute {
            id,
            values: values(pairs),
        },
    )
}
fn output(c: &AppCore, id: u64) -> usize {
    c.project()
        .current()
        .cells
        .iter()
        .find(|x| x.id == id)
        .unwrap()
        .output
        .as_ref()
        .unwrap()
        .root
        .unwrap()
}
fn rejected(c: &mut AppCore, command: Command) -> Error {
    let before = serde_json::to_value(c.project()).unwrap();
    let error = c
        .apply(Request {
            expected_revision: c.project().revision,
            actor: Actor::Human,
            command,
        })
        .unwrap_err();
    assert_eq!(serde_json::to_value(c.project()).unwrap(), before);
    error
}
#[test]
fn symbolic_evaluation_is_not_substitution_and_partial_equations_remain_equations() {
    let mut c = core();
    let id = define(&mut c, "surface", "z=x^2-y^2", true);
    let before = c.snapshot().cells[0].result_text.clone();
    edit(
        &mut c,
        Command::Transform {
            id,
            operation: "compute".into(),
            variable: "x".into(),
            bindings: BTreeMap::new(),
        },
    );
    assert_eq!(c.snapshot().cells[0].result_text, before);
    let s = substitute(&mut c, id, &[("x", "2")]);
    let cell = &s.cells[0];
    assert_eq!(cell.cell.source, "z=x^2-y^2");
    assert_eq!(cell.parameters, ["x", "y"]);
    let mathml = cell.result_mathml.as_ref().unwrap();
    assert!(!mathml.contains("<mn>0</mn>"));
    assert!(mathml.find("<mn>4</mn>").unwrap() < mathml.find("<mi>y</mi>").unwrap());
    assert_eq!(cell.dependent_symbol.as_deref(), Some("z"));
    assert_eq!(cell.cell.output.as_ref().unwrap().precision, "exact");
    assert_eq!(cell.cell.output.as_ref().unwrap().trust, Trust::Symbolic);
    assert!(
        cell.cell
            .output
            .as_ref()
            .unwrap()
            .conditions
            .contains(&"x = 2".to_string())
    );
    let Node::Equation(left, right) = c.project().graph.nodes[output(&c, id)] else {
        panic!("equation lost")
    };
    assert!(matches!(&c.project().graph.nodes[left],Node::Symbol(s) if s=="z"));
    assert_eq!(
        c.project()
            .graph
            .numeric(right, &BTreeMap::from([("y".into(), 3.)]))
            .unwrap(),
        -5.
    );
    assert!(!c.project().graph.symbols(right).contains("x"));
}
#[test]
fn decimals_and_fractions_are_exact_and_small_rational_polynomials_match_independent_arithmetic() {
    let mut c = core();
    let id = define(&mut c, "f", "x^2+y", false);
    substitute(&mut c, id, &[("x", "0.1"), ("y", "1/3")]);
    assert_eq!(
        c.project().graph.value(output(&c, id)).unwrap(),
        Rational::new(103.into(), 300.into())
    );
    for numerator in -3..=3 {
        let mut c = core();
        let id = define(&mut c, "f", "(x+1)^2", false);
        substitute(&mut c, id, &[("x", &format!("{numerator}/3"))]);
        let expected = Rational::new((numerator + 3).into(), 3.into());
        assert_eq!(
            c.project().graph.value(output(&c, id)).unwrap(),
            &expected * &expected
        );
    }
}
#[test]
fn substitutions_preserve_reference_recipes_through_recalculation_undo_and_file_roundtrip() {
    let mut c = core();
    let a = define(&mut c, "a", "x^2", false);
    let b = define(&mut c, "b", "@a+y", false);
    substitute(&mut c, b, &[("x", "2")]);
    let result = |c: &AppCore| {
        c.project()
            .graph
            .numeric(output(c, b), &BTreeMap::from([("y".into(), 1.)]))
            .unwrap()
    };
    assert_eq!(result(&c), 5.);
    edit(
        &mut c,
        Command::Define {
            id: Some(a),
            name: "a".into(),
            source: "x^3".into(),
        },
    );
    assert_eq!(result(&c), 9.);
    edit(&mut c, Command::Undo);
    assert_eq!(result(&c), 5.);
    edit(&mut c, Command::Redo);
    assert_eq!(result(&c), 9.);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("research.symbio");
    {
        let mut store = ProjectStore::open(&path).unwrap();
        store.save(c.project()).unwrap();
    }
    let mut store = ProjectStore::open(&path).unwrap();
    let mut restored = AppCore::new(store.load().unwrap().unwrap()).unwrap();
    assert_eq!(
        restored.project().current().cells[1]
            .calculation
            .as_ref()
            .unwrap()
            .exact_bindings,
        values(&[("x", "2")])
    );
    edit(
        &mut restored,
        Command::Define {
            id: Some(a),
            name: "a".into(),
            source: "x^4".into(),
        },
    );
    assert_eq!(result(&restored), 17.);
}
#[test]
fn singular_values_assumptions_and_unknown_keys_are_rejected_atomically() {
    let mut c = core();
    let id = define(&mut c, "f", "x/x", false);
    assert!(matches!(
        rejected(
            &mut c,
            Command::Substitute {
                id,
                values: values(&[("x", "0")])
            }
        ),
        Error::Domain(_)
    ));
    rejected(
        &mut c,
        Command::Substitute {
            id,
            values: values(&[("x", "1/0")]),
        },
    );
    rejected(
        &mut c,
        Command::Substitute {
            id,
            values: values(&[("absent", "2")]),
        },
    );
    let mut c = core();
    let id = define(&mut c, "f", "x", false);
    edit(
        &mut c,
        Command::Assume {
            symbol: "x".into(),
            assumption: Assumption {
                domain: Domain::Positive,
                ..Default::default()
            },
        },
    );
    assert!(matches!(
        rejected(
            &mut c,
            Command::Substitute {
                id,
                values: values(&[("x", "-1/3")])
            }
        ),
        Error::Domain(_)
    ));
    rejected(
        &mut c,
        Command::Substitute {
            id,
            values: values(&[("x", "0")]),
        },
    );
    edit(
        &mut c,
        Command::Assume {
            symbol: "x".into(),
            assumption: Assumption {
                dimension: Some([0, 1, 0]),
                ..Default::default()
            },
        },
    );
    assert!(matches!(
        rejected(
            &mut c,
            Command::Substitute {
                id,
                values: values(&[("x", "2")])
            }
        ),
        Error::Unsupported(_)
    ));
}
#[test]
fn bound_dummies_are_not_replaced_and_unreduced_calculus_is_rejected_explicitly() {
    let mut c = core();
    let id = define(&mut c, "area", r"\int_{0}^{x} t^{2}\,dt", true);
    substitute(&mut c, id, &[("x", "2")]);
    assert_eq!(
        c.project().graph.value(output(&c, id)).unwrap(),
        Rational::new(8.into(), 3.into())
    );
    let derivative = define(&mut c, "d", r"\frac{d}{dx}x^{2}", true);
    assert!(matches!(
        rejected(
            &mut c,
            Command::Substitute {
                id: derivative,
                values: values(&[("x", "2")])
            }
        ),
        Error::Unsupported(_)
    ));
}
#[test]
fn natural_reference_rendering_retains_machine_identity_and_raw_source_syntax() {
    let mut c = core();
    define(&mut c, "a", "4", false);
    let id = define(&mut c, "b", "@a+3", false);
    let s = c.snapshot();
    let cell = s.cells.iter().find(|c| c.cell.id == id).unwrap();
    assert!(cell.expression.contains("@a"));
    assert!(cell.mathml.contains("data-reference=\"a\""));
    assert!(!cell.mathml.contains("<mo>@</mo>"));
    assert!(cell.definition_mathml.contains("<mo>:=</mo>"));
    assert!(cell.definition_mathml.contains("<mi>b</mi>"));
    assert!(matches!(
        &c.project().graph.nodes[cell.cell.root],
        Node::Add(..)
    ));
}
#[test]
fn explicit_model_roles_do_not_hide_self_dependent_or_implicit_equations() {
    let mut c = core();
    define(&mut c, "implicit", "x^2=y^2", true);
    define(&mut c, "self", "z=z^2", true);
    for cell in c.snapshot().cells {
        assert!(cell.dependent_symbol.is_none());
        assert_eq!(cell.parameters, cell.symbols);
    }
    let preview = c.preview_latex("z=x^2-y^2", None).unwrap();
    assert_eq!(preview.parameters, ["x", "y"]);
    assert_eq!(preview.dependent_symbol.as_deref(), Some("z"));
}
#[test]
fn substitution_request_and_recipe_shapes_remain_closed_and_bounded() {
    assert!(
        serde_json::from_str::<Command>(
            r#"{"method":"math.substitute","id":1,"values":{"x":"1/3"},"unexpected":true}"#
        )
        .is_err()
    );
    assert!(substitution::validate(&BTreeMap::new()).is_err());
    assert!(substitution::literal("sqrt(2)").is_err());
    assert!(substitution::literal(&"1".repeat(257)).is_err());
}

#[test]
fn explicit_model_numeric_evaluation_does_not_require_an_input_value_for_its_output() {
    let mut c = core();
    let id = define(&mut c, "surface", "z=x^2-y^2", true);
    let snapshot = edit(
        &mut c,
        Command::Transform {
            id,
            operation: "evaluate".into(),
            variable: "x".into(),
            bindings: BTreeMap::from([("x".into(), 2.), ("y".into(), 3.)]),
        },
    );
    let cell = &snapshot.cells[0];
    assert_eq!(cell.cell.output.as_ref().unwrap().trust, Trust::Approximate);
    assert!(cell.result_mathml.as_ref().unwrap().contains("<mo>≈</mo>"));
    assert!(cell.result_mathml.as_ref().unwrap().contains("<mi>z</mi>"));
    let Node::Equation(_, right) = c.project().graph.nodes[output(&c, id)] else {
        panic!("output equation lost")
    };
    assert_eq!(
        c.project().graph.value(right).unwrap(),
        Rational::from_integer((-5).into())
    );
}
