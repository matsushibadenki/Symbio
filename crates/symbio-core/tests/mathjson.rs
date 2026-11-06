use serde_json::{Value, json};
use symbio_core::{mathjson::*, *};
fn load(value: Value) -> AppCore {
    import(&serde_json::to_vec(&value).unwrap()).unwrap()
}
fn edit(c: &mut AppCore, command: Command) {
    c.apply(Request {
        expected_revision: c.project().revision,
        actor: Actor::Human,
        command,
    })
    .unwrap();
}
fn envelope(expression: Value) -> Value {
    json!({"format":"symbio.mathjson","version":1,"name":"f","expression":expression,"assumptions":{}})
}
#[test]
fn integers_and_rationals_remain_exact_beyond_float64() {
    let c = load(json!(["Add",{"num":"9007199254740993"},["Rational",{"num":"1"},{"num":"3"}]]));
    let cell = &c.project().current().cells[0];
    assert_eq!(
        c.project()
            .graph
            .value(cell.output.as_ref().unwrap().root.unwrap())
            .unwrap(),
        graph::rational("27021597764222980/3").unwrap()
    );
    let e = export(c.project(), cell.id).unwrap();
    let restored = import(&to_bytes(&e).unwrap()).unwrap();
    assert_eq!(
        c.snapshot().cells[0].result_text,
        restored.snapshot().cells[0].result_text
    );
    let neg = load(json!(["Rational", -3, 4]));
    assert_eq!(neg.snapshot().cells[0].result_text.as_deref(), Some("-3/4"));
}
#[test]
fn assumptions_are_preserved_in_context_and_never_inferred_for_bare_input() {
    let expression = json!(["Sqrt", ["Power", "x", 2]]);
    let bare = load(expression.clone());
    assert!(bare.project().current().assumptions.is_empty());
    assert_ne!(
        bare.snapshot().cells[0].result_text.as_deref(),
        Some("abs(x)")
    );
    let mut v = envelope(expression);
    v["assumptions"] = json!({"x":{"domain":"real","nonzero":false,"dimension":null}});
    let real = load(v);
    assert_eq!(
        real.snapshot().cells[0].result_text.as_deref(),
        Some("abs(x)")
    );
    let e = export(real.project(), 1).unwrap();
    assert_eq!(e.assumptions["x"].domain, Domain::Real);
    let copy = import(&to_bytes(&e).unwrap()).unwrap();
    assert_eq!(
        copy.project().current().assumptions,
        real.project().current().assumptions
    );
}
#[test]
fn references_export_as_standalone_definitions_without_changing_owner() {
    let mut c = AppCore::new(Project::empty("Original").unwrap()).unwrap();
    for (name, source) in [("a", "4"), ("b", "@a+x")] {
        edit(
            &mut c,
            Command::Define {
                id: None,
                name: name.into(),
                source: source.into(),
            },
        );
    }
    let before = serde_json::to_value(c.snapshot()).unwrap();
    let e = export(c.project(), 2).unwrap();
    assert!(!e.expression.to_string().contains('@'));
    let copy = import(&to_bytes(&e).unwrap()).unwrap();
    assert_eq!(copy.project().current().cells.len(), 1);
    assert!(copy.snapshot().dependencies.is_empty());
    assert_eq!(
        copy.snapshot().cells[0].result_text,
        c.snapshot().cells[1].result_text
    );
    assert_eq!(before, serde_json::to_value(c.snapshot()).unwrap());
}
#[test]
fn nested_lists_are_matrices_and_multiply_uses_matrix_product() {
    let a = json!(["List", ["List", 1, 2], ["List", 3, 4]]);
    let c = load(json!(["Multiply", a.clone(), a]));
    let s = c.snapshot();
    assert_eq!(s.cells[0].kind, "matrix");
    assert!(s.cells[0].result_text.as_ref().unwrap().contains("22"));
    let e = export(c.project(), 1).unwrap();
    let mut copy = import(&to_bytes(&e).unwrap()).unwrap();
    edit(
        &mut copy,
        Command::Transform {
            id: 1,
            operation: "determinant".into(),
            variable: "x".into(),
            bindings: Default::default(),
        },
    );
    assert_eq!(copy.snapshot().cells[0].result_text.as_deref(), Some("4"));
}
#[test]
fn explicit_equations_can_be_solved_after_import() {
    let mut c = load(json!(["Equal", ["Power", "x", 2], 4]));
    assert_eq!(c.snapshot().cells[0].kind, "equation");
    edit(
        &mut c,
        Command::Transform {
            id: 1,
            operation: "solve".into(),
            variable: "x".into(),
            bindings: Default::default(),
        },
    );
    let result = c.snapshot().cells[0].result_text.clone().unwrap();
    assert!(result.contains("-2"));
    assert!(result.contains('2'));
}
#[test]
fn objects_functions_symbols_negation_and_constants_have_explicit_meanings() {
    for v in [
        json!({"fn":["Add",{"sym":"alpha"},{"num":"2"}]}),
        json!(["Negate", "x"]),
        json!(["Ln", "x"]),
        json!(["Sin", "x"]),
        json!(["Exp", "x"]),
        json!("Pi"),
        json!("ExponentialE"),
        json!("e"),
    ] {
        let c = load(v);
        assert!(c.snapshot().cells[0].cell.output.is_some());
    }
    assert_eq!(load(json!("e")).snapshot().cells[0].parameters, vec!["e"]);
    for head in [
        "Log",
        "Integrate",
        "Derivative",
        "Apply",
        "Run",
        "HadamardProduct",
        "Unknown",
    ] {
        assert!(
            import(&serde_json::to_vec(&json!([head, "x"])).unwrap()).is_err(),
            "{head}"
        );
    }
}
#[test]
fn unsupported_or_ambiguous_numbers_metadata_shapes_and_contexts_fail() {
    for v in [
        json!(0.1),
        json!({"num":"0.1"}),
        json!({"num":"1e50"}),
        json!("Infinity"),
        json!("NaN"),
        json!("GoldenRatio"),
        json!({"sym":2}),
        json!({"sym":"123"}),
        json!({"sym":"x","latex":"x"}),
        json!(["Rational", 1, 0]),
        json!(["List", 1, 2]),
        json!(["List", ["List", 1], ["List", 2, 3]]),
        json!(["Add", 1]),
        json!({"str":"x"}),
        json!("半径"),
    ] {
        assert!(import(&serde_json::to_vec(&v).unwrap()).is_err(), "{v}");
    }
    let mut v = envelope(json!(1));
    v["version"] = 2.into();
    assert!(import(&serde_json::to_vec(&v).unwrap()).is_err());
    v["version"] = 1.into();
    v["extra"] = true.into();
    assert!(import(&serde_json::to_vec(&v).unwrap()).is_err());
}
#[test]
fn bounds_and_no_clobber_are_enforced() {
    assert!(import(&vec![b' '; MAX_BYTES + 1]).is_err());
    let mut v = json!("x");
    for _ in 0..50 {
        v = json!(["Sqrt", v]);
    }
    assert!(import(&serde_json::to_vec(&v).unwrap()).is_err());
    let c = load(json!(["Add", "x", 1]));
    let e = export(c.project(), 1).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.json");
    write_new(&path, &e).unwrap();
    let before = read(&path).unwrap();
    assert!(write_new(&path, &e).is_err());
    assert_eq!(before, read(&path).unwrap());
}
#[test]
fn calculus_exports_authored_binder_instead_of_its_cached_result() {
    let mut c = AppCore::new(Project::empty("Calculus").unwrap()).unwrap();
    edit(
        &mut c,
        Command::DefineLatex {
            id: None,
            name: "f".into(),
            source: r"\int_0^1 x^2\,dx".into(),
        },
    );
    let e = export(c.project(), 1).unwrap();
    assert_eq!(
        e.expression,
        json!(["Integrate", ["Power", "x", {"num":"2"}], ["Tuple", "x", {"num":"0"}, {"num":"1"}]])
    );
    let copy = import(&to_bytes(&e).unwrap()).unwrap();
    assert_eq!(copy.snapshot().cells[0].result_text.as_deref(), Some("1/3"));
    let c = load(json!(["Divide", 1, 0]));
    assert_eq!(
        c.snapshot().cells[0].cell.calculation_error.as_deref(),
        Some("domain_error")
    );
}

#[test]
fn calculus_forms_preserve_binders_and_compute_exactly() {
    for (v, expected) in [
        (json!(["D", ["Power", "x", 3], "x"]), "(3 * (x ^ 2))"),
        (
            json!(["Integrate", ["Power", "x", 2], ["Tuple", "x", 0, 2]]),
            "8/3",
        ),
        (
            json!(["Sum", ["Power", "k", 2], ["Tuple", "k", 1, 3]]),
            "14",
        ),
        (json!(["Product", "k", ["Tuple", "k", 1, 4]]), "24"),
        (json!(["Sum", "k", ["Tuple", "k", 3, 1]]), "0"),
        (json!(["Product", "k", ["Tuple", "k", 3, 1]]), "1"),
    ] {
        let c = load(v);
        assert_eq!(c.snapshot().cells[0].result_text.as_deref(), Some(expected));
        let e = export(c.project(), 1).unwrap();
        let copy = import(&to_bytes(&e).unwrap()).unwrap();
        assert_eq!(
            copy.snapshot().cells[0].result_text,
            c.snapshot().cells[0].result_text
        );
        assert_eq!(export(copy.project(), 1).unwrap().expression, e.expression);
    }
}
#[test]
fn calculus_structure_scope_and_context_survive_roundtrip() {
    let v = json!([
        "Sum",
        ["Integrate", ["Add", "x", "k"], ["Tuple", "x", 0, "y"]],
        ["Tuple", "k", 1, 3]
    ]);
    let c = load(v);
    let s = c.snapshot();
    assert_eq!(s.cells[0].parameters, vec!["y"]);
    let e = export(c.project(), 1).unwrap();
    let copy = import(&to_bytes(&e).unwrap()).unwrap();
    assert_eq!(export(copy.project(), 1).unwrap().expression, e.expression);
    // An unsupported evaluation must retain its authored mathematical object.
    let c = load(json!(["Integrate", ["Sin", ["Power", "x", 2]], "x"]));
    assert_eq!(
        c.snapshot().cells[0].cell.calculation_error.as_deref(),
        Some("unsupported")
    );
    assert!(
        export(c.project(), 1)
            .unwrap()
            .expression
            .to_string()
            .contains("Integrate")
    );
    // Bare D remains a physical symbol, while [D, body, variable] is an operator.
    assert_eq!(load(json!("D")).snapshot().cells[0].parameters, vec!["D"]);
    let c = load(json!(["D", ["D", ["Power", "x", 3], "x"], "x"]));
    assert!(c.snapshot().cells[0].cell.source.contains("\\frac"));
}
#[test]
fn ambiguous_calculus_and_invalid_shapes_are_rejected() {
    for v in [
        json!(["D", "x"]),
        json!(["D", "x", 2]),
        json!(["D", "x", "Pi"]),
        json!(["Derivative", "x", "x"]),
        json!(["Integrate", "x", ["Tuple", 0, 1]]),
        json!(["Integrate", "x", ["Tuple", "x", 0]]),
        json!(["Integrate", "x", "x", "y"]),
        json!(["Sum", "k", ["Tuple", "k", 1, 3, 2]]),
        json!(["Sum", "k", "k"]),
        json!(["Integrate", "x", ["Tuple", "x", 0, "Infinity"]]),
        json!(["D", ["List", ["List", 1]], "x"]),
        json!(["Product", "x", ["Tuple", "x", ["Equal", "a", "b"], 3]]),
    ] {
        assert!(import(&serde_json::to_vec(&v).unwrap()).is_err(), "{v}");
    }
    assert!(matches!(
        import(&serde_json::to_vec(&json!(["Sum", "k", ["Tuple", "k", 1, 300]])).unwrap()),
        Err(Error::Limit)
    ));
    let c = load(json!(["Sum", "k", ["Tuple", "k", ["Rational", 1, 2], 3]]));
    assert_eq!(
        c.snapshot().cells[0].cell.calculation_error.as_deref(),
        Some("unsupported")
    );
}

#[test]
fn calculus_variable_objects_and_bounds_keep_free_parameters() {
    for v in [
        json!({"fn":["Integrate", {"fn":["Power", "x", 2]}, {"sym":"x"}]}),
        json!(["D", ["Power", "theta", 3], {"sym":"theta"}]),
        json!(["Sum", "k", {"fn":["Tuple", {"sym":"k"}, "k", 3]}]),
        json!([
            "Sum",
            ["Sum", "k", ["Tuple", "k", 1, 2]],
            ["Tuple", "k", 1, 3]
        ]),
    ] {
        let c = load(v);
        let e = export(c.project(), 1).unwrap();
        let copy = import(&to_bytes(&e).unwrap()).unwrap();
        assert_eq!(e.expression, export(copy.project(), 1).unwrap().expression);
        assert_eq!(
            c.snapshot().cells[0].parameters,
            copy.snapshot().cells[0].parameters
        );
    }
    let c = load(json!(["Sum", "k", ["Tuple", "k", "k", 3]]));
    assert_eq!(c.snapshot().cells[0].parameters, vec!["k"]);
    let mut v = envelope(json!(["Integrate", ["Power", "x", 2], "x"]));
    v["assumptions"] = json!({"x":{"domain":"positive","nonzero":true,"dimension":null}});
    let c = load(v);
    let e = export(c.project(), 1).unwrap();
    assert_eq!(e.assumptions["x"].domain, Domain::Positive);
    assert!(
        c.snapshot().cells[0]
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
fn binder_operands_do_not_capture_surrounding_operations_in_generated_latex() {
    for v in [
        json!(["Add", ["D", ["Power", "x", 2], "x"], "x"]),
        json!(["Multiply", ["D", ["Power", "x", 2], "x"], "x"]),
        json!(["Power", ["D", ["Power", "x", 2], "x"], 2]),
        json!(["Multiply", ["Sum", "k", ["Tuple", "k", 1, 3]], "k"]),
        json!(["Power", ["Product", "k", ["Tuple", "k", 1, 3]], 2]),
        json!(["Multiply", ["Integrate", ["Power", "x", 2], "x"], "x"]),
        json!([
            "Add",
            ["Integrate", ["Power", "x", 2], ["Tuple", "x", 0, 2]],
            "x"
        ]),
    ] {
        let mut g = graph::Graph::default();
        let root = decode(&mut g, &v).unwrap();
        let expected = encode(&g, root).unwrap();
        let c = load(v.clone());
        assert_eq!(export(c.project(), 1).unwrap().expression, expected, "{v}");
    }
    let c = load(json!(["Add", ["D", ["Power", "x", 2], "x"], "x"]));
    assert_eq!(
        c.snapshot().cells[0].result_text.as_deref(),
        Some("(3 * x)")
    );
}

#[test]
fn determinant_and_transpose_preserve_authored_matrix_operations() {
    let m = json!(["List", ["List", 1, 2], ["List", 3, 4]]);
    for v in [
        json!(["Determinant", m]),
        json!(["Transpose", m]),
        json!(["Determinant", ["Transpose", m]]),
        json!(["Transpose", ["Transpose", m]]),
        json!(["Transpose", ["List", ["List", 1, 2, 3], ["List", 4, 5, 6]]]),
    ] {
        let c = load(v);
        let e = export(c.project(), 1).unwrap();
        let copy = import(&to_bytes(&e).unwrap()).unwrap();
        assert_eq!(e.expression, export(copy.project(), 1).unwrap().expression);
        assert_eq!(
            c.snapshot().cells[0].result_text,
            copy.snapshot().cells[0].result_text
        );
        assert!(copy.snapshot().cells[0].cell.output.is_some());
    }
    assert_eq!(
        load(json!(["Determinant", m])).snapshot().cells[0]
            .result_text
            .as_deref(),
        Some("-2")
    );
    assert_eq!(
        load(json!([
            "Determinant",
            ["List", ["List", 1, 2], ["List", 2, 4]]
        ]))
        .snapshot()
        .cells[0]
            .result_text
            .as_deref(),
        Some("0")
    );
    for v in [
        json!(["Determinant", 5]),
        json!(["Transpose", "x"]),
        json!(["Determinant", ["List", ["List", 1, 2, 3]]]),
        json!(["Transpose", m, 1, 2]),
        json!(["Determinant", m, 0]),
        json!(["ConjugateTranspose", m]),
    ] {
        assert!(import(&serde_json::to_vec(&v).unwrap()).is_err(), "{v}");
    }
}

#[test]
fn matrix_exchange_results_have_independently_checked_entries_and_rationals() {
    let c = load(json!([
        "Transpose",
        ["List", ["List", 1, 2, 3], ["List", 4, 5, 6]]
    ]));
    let root = c.project().current().cells[0]
        .output
        .as_ref()
        .unwrap()
        .root
        .unwrap();
    let graph::Node::Matrix {
        rows,
        cols,
        entries,
    } = &c.project().graph.nodes[root]
    else {
        panic!("matrix result required")
    };
    assert_eq!((*rows, *cols), (3, 2));
    let values: Vec<_> = entries
        .iter()
        .map(|&id| graph::rat_text(&c.project().graph.value(id).unwrap()))
        .collect();
    assert_eq!(values, ["1", "4", "2", "5", "3", "6"]);
    let c = load(json!([
        "Determinant",
        [
            "List",
            ["List", ["Rational", 1, 2], ["Rational", 1, 3]],
            ["List", ["Rational", 2, 3], ["Rational", 3, 4]]
        ]
    ]));
    assert_eq!(c.snapshot().cells[0].result_text.as_deref(), Some("11/72"));
}

#[test]
fn explicit_matrix_product_keeps_order_shape_and_authored_operation() {
    let a = json!(["List", ["List", 1, 2], ["List", 3, 4]]);
    let b = json!(["List", ["List", 5, 6], ["List", 7, 8]]);
    let c = load(json!(["MatrixMultiply", a, b]));
    assert_eq!(
        c.snapshot().cells[0].result_text.as_deref(),
        Some("matrix(2x2; 19, 22, 43, 50)")
    );
    let e = export(c.project(), 1).unwrap();
    assert_eq!(e.expression[0], "MatrixMultiply");
    let copy = import(&to_bytes(&e).unwrap()).unwrap();
    assert_eq!(
        c.snapshot().cells[0].result_text,
        copy.snapshot().cells[0].result_text
    );
    assert_ne!(
        c.snapshot().cells[0].result_text,
        load(json!(["MatrixMultiply", b, a])).snapshot().cells[0].result_text
    );
    for v in [
        json!(["MatrixMultiply", 2, a]),
        json!(["MatrixMultiply", a]),
        json!(["MatrixMultiply", a, b, a]),
        json!(["MatrixMultiply", a, ["List", ["List", 1, 2, 3]]]),
        json!(["HadamardProduct", a, b]),
    ] {
        assert!(import(&serde_json::to_vec(&v).unwrap()).is_err());
    }
}

#[test]
fn inverse_matrix_exchange_keeps_exact_values_and_domain_failures() {
    let m = json!(["List", ["List", 1, 2], ["List", 3, 4]]);
    let c = load(json!(["Inverse", m]));
    assert_eq!(
        c.snapshot().cells[0].result_text.as_deref(),
        Some("matrix(2x2; -2, 1, 3/2, -1/2)")
    );
    let e = export(c.project(), 1).unwrap();
    let copy = import(&to_bytes(&e).unwrap()).unwrap();
    assert_eq!(
        copy.snapshot().cells[0].result_text,
        c.snapshot().cells[0].result_text
    );
    let singular = load(json!(["Inverse", ["List", ["List", 1, 2], ["List", 2, 4]]]));
    assert_eq!(
        singular.snapshot().cells[0]
            .cell
            .calculation_error
            .as_deref(),
        Some("domain_error")
    );
    let e = export(singular.project(), 1).unwrap();
    let copy = import(&to_bytes(&e).unwrap()).unwrap();
    assert_eq!(
        copy.snapshot().cells[0].cell.calculation_error.as_deref(),
        Some("domain_error")
    );
    for v in [
        json!(["Inverse", 2]),
        json!(["Inverse", "x"]),
        json!(["Inverse", ["List", ["List", 1, 2]]]),
        json!(["Inverse", m, 2]),
    ] {
        assert!(import(&serde_json::to_vec(&v).unwrap()).is_err());
    }
}
