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
fn unsupported_seg_nodes_are_not_silently_stringified() {
    let mut c = AppCore::new(Project::empty("Calculus").unwrap()).unwrap();
    edit(
        &mut c,
        Command::DefineLatex {
            id: None,
            name: "f".into(),
            source: r"\int_0^1 x^2\,dx".into(),
        },
    );
    assert!(export(c.project(), 1).is_err());
    let c = load(json!(["Divide", 1, 0]));
    assert_eq!(
        c.snapshot().cells[0].cell.calculation_error.as_deref(),
        Some("domain_error")
    );
}
