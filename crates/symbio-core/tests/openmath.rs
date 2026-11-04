use symbio_core::{openmath::*, *};
fn object(body: &str) -> String {
    format!(r#"<OMOBJ xmlns="http://www.openmath.org/OpenMath" version="2.0">{body}</OMOBJ>"#)
}
fn load(body: &str) -> AppCore {
    import(object(body).as_bytes()).unwrap()
}
fn edit(c: &mut AppCore, command: Command) {
    c.apply(Request {
        expected_revision: c.project().revision,
        actor: Actor::Human,
        command,
    })
    .unwrap();
}
#[test]
fn exact_numbers_functions_and_standard_symbols_roundtrip() {
    for body in [
        r#"<OMA><OMS cd="arith1" name="plus"/><OMI>9007199254740993</OMI><OMA><OMS cd="nums1" name="rational"/><OMI>1</OMI><OMI>3</OMI></OMA></OMA>"#,
        r#"<OMA><OMS cd="arith1" name="root"/><OMI>2</OMI><OMI>2</OMI></OMA>"#,
        r#"<OMA><OMS cd="transc1" name="ln"/><OMV name="x"/></OMA>"#,
        r#"<OMA><OMS cd="rounding1" name="floor"/><OMA><OMS cd="nums1" name="rational"/><OMI>-3</OMI><OMI>2</OMI></OMA></OMA>"#,
        r#"<OMS cd="nums1" name="pi"/>"#,
        r#"<OMS cd="nums1" name="e"/>"#,
    ] {
        let c = load(body);
        let bytes = export(c.project(), 1).unwrap();
        let copy = import(&bytes).unwrap();
        assert_eq!(
            c.snapshot().cells[0].result_text,
            copy.snapshot().cells[0].result_text
        );
        assert!(export(copy.project(), 1).is_ok());
        assert_eq!(
            c.snapshot().cells[0].parameters,
            copy.snapshot().cells[0].parameters
        );
    }
    let c = load(r#"<OMI>+0009007199254740993</OMI>"#);
    assert_eq!(
        c.snapshot().cells[0].result_text.as_deref(),
        Some("9007199254740993")
    );
}
#[test]
fn context_preserves_assumptions_and_reference_definitions_without_mutation() {
    let mut c = AppCore::new(Project::empty("Owner").unwrap()).unwrap();
    edit(
        &mut c,
        Command::Assume {
            symbol: "x".into(),
            assumption: Assumption {
                domain: Domain::Real,
                ..Default::default()
            },
        },
    );
    edit(
        &mut c,
        Command::Define {
            id: None,
            name: "a".into(),
            source: "sqrt(x^2)".into(),
        },
    );
    edit(
        &mut c,
        Command::Define {
            id: None,
            name: "b".into(),
            source: "@a+1/3".into(),
        },
    );
    let before = serde_json::to_value(c.snapshot()).unwrap();
    let bytes = export(c.project(), 2).unwrap();
    let copy = import(&bytes).unwrap();
    assert_eq!(
        copy.snapshot().cells[0].result_text,
        c.snapshot().cells[1].result_text
    );
    assert_eq!(
        copy.project().current().assumptions,
        c.project().current().assumptions
    );
    assert_eq!(copy.snapshot().cells[0].cell.name, "b");
    assert!(copy.snapshot().dependencies.is_empty());
    assert_eq!(before, serde_json::to_value(c.snapshot()).unwrap());
    let bare = load(
        r#"<OMA><OMS cd="arith1" name="root"/><OMA><OMS cd="arith1" name="power"/><OMV name="x"/><OMI>2</OMI></OMA><OMI>2</OMI></OMA>"#,
    );
    assert!(bare.project().current().assumptions.is_empty());
    assert_ne!(
        bare.snapshot().cells[0].result_text.as_deref(),
        Some("abs(x)")
    );
    let xml = String::from_utf8(bytes).unwrap();
    assert!(xml.contains("&quot;"));
    assert!(import(xml.replace("symbio_context1", "unknown_context").as_bytes()).is_err());
    assert!(
        import(
            xml.replace("&quot;version&quot;:1", "&quot;version&quot;:2")
                .as_bytes()
        )
        .is_err()
    );
}
#[test]
fn equations_and_matrix_constructors_keep_their_math_types() {
    let mut c = load(
        r#"<OMA><OMS cd="relation1" name="eq"/><OMA><OMS cd="arith1" name="power"/><OMV name="x"/><OMI>2</OMI></OMA><OMI>4</OMI></OMA>"#,
    );
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
    assert!(
        c.snapshot().cells[0]
            .result_text
            .as_ref()
            .unwrap()
            .contains("-2")
    );
    let c = load(
        r#"<OMA><OMS cd="linalg2" name="matrix"/><OMA><OMS cd="linalg2" name="matrixrow"/><OMI>1</OMI><OMI>2</OMI></OMA><OMA><OMS cd="linalg2" name="matrixrow"/><OMI>3</OMI><OMI>4</OMI></OMA></OMA>"#,
    );
    assert_eq!(c.snapshot().cells[0].kind, "matrix");
    let bytes = export(c.project(), 1).unwrap();
    assert_eq!(
        import(&bytes).unwrap().snapshot().cells[0].result_text,
        c.snapshot().cells[0].result_text
    );
    let c = load(r#"<OMA><OMS cd="arith1" name="divide"/><OMI>1</OMI><OMI>0</OMI></OMA>"#);
    assert_eq!(
        c.snapshot().cells[0].cell.calculation_error.as_deref(),
        Some("domain_error")
    );
    assert!(export(c.project(), 1).is_ok());
}
#[test]
fn xml_and_unknown_semantics_are_rejected_without_resolution() {
    for body in [
        r#"<OMS cd="unknown" name="pi"/>"#,
        r#"<OMS cdbase="https://evil.invalid" cd="nums1" name="pi"/>"#,
        r#"<OMS cdbase="urn:symbio:cd" cd="nums1" name="pi"/>"#,
        r#"<OMF dec="0.1"/>"#,
        r#"<OMR href="file:///etc/passwd"/>"#,
        r#"<OMV name="x" name="y"/>"#,
        r#"<OMV name="x" xmlns="evil"/>"#,
        r#"<OMI>1e3</OMI>"#,
        r#"<OMATTR><OMATP><OMS cd="unknown" name="type"/><OMI>1</OMI></OMATP><OMV name="x"/></OMATTR>"#,
        r#"<OMA><OMS cd="arith1" name="plus"/><OMI>1</OMI></OMA>"#,
        r#"<OMA><OMV name="f"/><OMI>1</OMI></OMA>"#,
        r#"<OMI>&xxe;</OMI>"#,
        r#"<OMI><![CDATA[2]]></OMI>"#,
        r#"<OMV name="x" xml:base="http://evil.invalid"/>"#,
        r#"<OMBIND><OMS cd="fns1" name="lambda"/><OMBVAR><OMV name="x"/></OMBVAR><OMV name="x"/></OMBIND>"#,
    ] {
        assert!(import(object(body).as_bytes()).is_err(), "{body}");
    }
    for xml in [
        "<!DOCTYPE OMOBJ [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><OMOBJ/>",
        "<OMOBJ>",
        "<OMOBJ xmlns='evil'><OMI>1</OMI></OMOBJ>",
        "<?xml version='1.1'?><OMOBJ/>",
        "<OMOBJ/><OMOBJ/>",
    ] {
        assert!(import(xml.as_bytes()).is_err());
    }
    assert!(import(&vec![b' '; MAX_BYTES + 1]).is_err());
    let deep = format!("{}<OMI>1</OMI>{}", "<OMA>".repeat(50), "</OMA>".repeat(50));
    assert!(matches!(
        import(object(&deep).as_bytes()),
        Err(Error::Limit)
    ));
}
#[test]
fn export_scope_and_atomic_no_clobber_are_explicit() {
    let mut c = AppCore::new(Project::empty("Scope").unwrap()).unwrap();
    for source in [
        r"\begin{bmatrix}1\end{bmatrix}^2",
        r"\begin{bmatrix}1\end{bmatrix}+\begin{bmatrix}2\end{bmatrix}",
    ] {
        let id = (!c.project().current().cells.is_empty()).then_some(1);
        edit(
            &mut c,
            Command::DefineLatex {
                id,
                name: "f".into(),
                source: source.into(),
            },
        );
        assert!(export(c.project(), 1).is_err());
    }
    let c = load("<OMI>7</OMI>");
    let bytes = export(c.project(), 1).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("math.xml");
    write_new(&path, &bytes).unwrap();
    assert!(write_new(&path, b"replacement").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let xml = object(r#"<OMA><OMS cd="arith1" name="plus"/>&#x20;<OMI>1</OMI><OMI>2</OMI></OMA>"#);
    assert_eq!(
        import(xml.as_bytes()).unwrap().snapshot().cells[0]
            .result_text
            .as_deref(),
        Some("3")
    );
}

#[test]
fn namespaces_are_resolved_and_foreign_dictionary_bases_are_not_reinterpreted() {
    let xml = r#"<om:OMOBJ xmlns:om="http://www.openmath.org/OpenMath" cdbase="http://www.openmath.org/cd"><om:OMA><om:OMS cd="arith1" name="plus"/><om:OMI>1</om:OMI><om:OMI>2</om:OMI></om:OMA></om:OMOBJ>"#;
    assert_eq!(
        import(xml.as_bytes()).unwrap().snapshot().cells[0]
            .result_text
            .as_deref(),
        Some("3")
    );
    assert!(
        import(
            xml.replace(
                "cdbase=\"http://www.openmath.org/cd\"",
                "cdbase=\"urn:foreign\""
            )
            .as_bytes()
        )
        .is_err()
    );
    assert!(
        import(
            xml.replace("<om:OMI>1", "<om:OMI xmlns:om=\"urn:foreign\">1")
                .as_bytes()
        )
        .is_err()
    );
}

fn from_mathjson(v: serde_json::Value) -> AppCore {
    symbio_core::mathjson::import(&serde_json::to_vec(&v).unwrap()).unwrap()
}
fn roundtrip(c: &AppCore) -> AppCore {
    let before = serde_json::to_value(c.snapshot()).unwrap();
    let xml = export(c.project(), 1).unwrap();
    let copy = import(&xml).unwrap();
    assert_eq!(before, serde_json::to_value(c.snapshot()).unwrap());
    assert_eq!(
        symbio_core::mathjson::export(c.project(), 1)
            .unwrap()
            .expression,
        symbio_core::mathjson::export(copy.project(), 1)
            .unwrap()
            .expression
    );
    assert_eq!(
        c.snapshot().cells[0].result_text,
        copy.snapshot().cells[0].result_text
    );
    assert_eq!(
        c.snapshot().cells[0].parameters,
        copy.snapshot().cells[0].parameters
    );
    copy
}
#[test]
fn calculus_exports_authored_binders_and_computes_exact_results() {
    use serde_json::json;
    for (expression, result, operator) in [
        (
            json!(["D", ["Power", "x", 3], "x"]),
            "(3 * (x ^ 2))",
            "diff",
        ),
        (
            json!(["Integrate", ["Power", "x", 2], ["Tuple", "x", 0, 2]]),
            "8/3",
            "defint",
        ),
        (
            json!(["Integrate", ["Power", "x", 2], ["Tuple", "x", 2, 0]]),
            "-8/3",
            "defint",
        ),
        (
            json!(["Sum", ["Divide", 1, "k"], ["Tuple", "k", 1, 3]]),
            "11/6",
            "sum",
        ),
        (
            json!(["Product", "k", ["Tuple", "k", 1, 4]]),
            "24",
            "product",
        ),
        (json!(["Sum", "k", ["Tuple", "k", 3, 1]]), "0", "sum"),
        (
            json!(["Product", "k", ["Tuple", "k", 3, 1]]),
            "1",
            "product",
        ),
    ] {
        let c = from_mathjson(expression);
        let xml = String::from_utf8(export(c.project(), 1).unwrap()).unwrap();
        assert!(xml.contains(&format!("name=\"{operator}\"")));
        assert!(xml.contains("<OMBIND><OMS cd=\"fns1\" name=\"lambda\"/>"));
        assert!(xml.contains("<OMBVAR><OMV name="));
        assert_eq!(
            roundtrip(&c).snapshot().cells[0].result_text.as_deref(),
            Some(result)
        );
    }
}
#[test]
fn calculus_scope_bounds_and_surrounding_operations_survive_xml() {
    use serde_json::json;
    for expression in [
        json!([
            "Sum",
            ["Integrate", ["Add", "x", "k"], ["Tuple", "x", 0, "y"]],
            ["Tuple", "k", 1, 3]
        ]),
        json!(["Sum", "k", ["Tuple", "k", "k", 3]]),
        json!([
            "Sum",
            ["Sum", "k", ["Tuple", "k", 1, 2]],
            ["Tuple", "k", 1, 3]
        ]),
        json!(["Integrate", "x", ["Tuple", "x", 0, "x"]]),
        json!(["D", ["D", ["Power", "x", 3], "x"], "x"]),
        json!(["Multiply", ["Integrate", ["Power", "x", 2], "x"], "x"]),
        json!(["Add", ["D", ["Power", "x", 2], "x"], "x"]),
        json!(["Multiply", ["Sum", "k", ["Tuple", "k", 1, 3]], "k"]),
    ] {
        roundtrip(&from_mathjson(expression));
    }
    let c = roundtrip(&from_mathjson(json!([
        "Integrate",
        "x",
        ["Tuple", "x", 0, "x"]
    ])));
    assert_eq!(c.snapshot().cells[0].parameters, vec!["x"]);
}
#[test]
fn primitive_conditions_context_and_unsupported_evaluation_are_retained() {
    use serde_json::json;
    let mut c = from_mathjson(json!(["Integrate", ["Power", "x", 2], "x"]));
    edit(
        &mut c,
        Command::Assume {
            symbol: "x".into(),
            assumption: Assumption {
                domain: Domain::Positive,
                nonzero: true,
                ..Default::default()
            },
        },
    );
    let copy = roundtrip(&c);
    assert_eq!(
        copy.project().current().assumptions,
        c.project().current().assumptions
    );
    assert!(
        copy.snapshot().cells[0]
            .cell
            .output
            .as_ref()
            .unwrap()
            .conditions
            .iter()
            .any(|s| s.contains("constant C"))
    );
    let c = roundtrip(&from_mathjson(json!([
        "Integrate",
        ["Sin", ["Power", "x", 2]],
        "x"
    ])));
    assert_eq!(
        c.snapshot().cells[0].cell.calculation_error.as_deref(),
        Some("unsupported")
    );
    let c = roundtrip(&from_mathjson(json!([
        "Integrate",
        ["Divide", 1, ["Subtract", "x", 1]],
        ["Tuple", "x", 0, 2]
    ])));
    assert!(c.snapshot().cells[0].cell.calculation_error.is_some());
}
#[test]
fn xml_binder_shapes_and_function_application_are_never_guessed() {
    let primitive = r#"<OMA><OMS cd="calculus1" name="int"/><OMBIND><OMS cd="fns1" name="lambda"/><OMBVAR><OMV name="x"/></OMBVAR><OMV name="x"/></OMBIND></OMA>"#;
    let applied = format!("<OMA>{primitive}<OMV name=\"x\"/></OMA>");
    assert!(import(object(&applied).as_bytes()).is_ok());
    for body in [
        primitive.to_string(), // a function is not its value
        applied.replace("</OMA><OMV name=\"x\"/>", "</OMA><OMV name=\"y\"/>"),
        applied.replace("</OMA><OMV name=\"x\"/>", "</OMA><OMI>2</OMI>"),
        applied.replace("<OMBVAR><OMV name=\"x\"/></OMBVAR>", "<OMBVAR/>"),
        applied.replace(
            "<OMBVAR><OMV name=\"x\"/></OMBVAR>",
            "<OMBVAR><OMV name=\"x\"/><OMV name=\"y\"/></OMBVAR>",
        ),
        applied.replace("cd=\"fns1\"", "cd=\"foreign\""),
        applied.replace("cd=\"fns1\"", "cdbase=\"urn:symbio:cd\" cd=\"fns1\""),
        applied.replace("<OMBVAR>", "<OMBVAR id=\"ignore\">"),
        applied.replace("<OMV name=\"x\"/></OMBVAR>", "<OMV name=\"Pi\"/></OMBVAR>"),
        applied.replace("name=\"int\"", "name=\"partialdiff\""),
    ] {
        assert!(import(object(&body).as_bytes()).is_err(), "{body}");
    }
    let bounded = r#"<OMA><OMS cd="calculus1" name="defint"/><OMA><OMS cd="interval1" name="oriented_interval"/><OMI>0</OMI><OMI>1</OMI></OMA><OMBIND><OMS cd="fns1" name="lambda"/><OMBVAR><OMV name="x"/></OMBVAR><OMV name="x"/></OMBIND></OMA>"#;
    assert_eq!(
        load(bounded).snapshot().cells[0].result_text.as_deref(),
        Some("1/2")
    );
    for body in [
        bounded.replace("oriented_interval", "interval_cc"),
        bounded.replace("oriented_interval", "ordered_interval"),
        bounded.replace("oriented_interval", "integer_interval"),
        bounded.replace("cd=\"interval1\"", "cd=\"foreign\""),
        bounded.replace("<OMI>1</OMI>", "<OMS cd=\"nums1\" name=\"infinity\"/>"),
        bounded.replace("<OMV name=\"x\"/></OMBVAR>", "<OMI>1</OMI></OMBVAR>"),
        bounded.replace(
            "<OMS cd=\"calculus1\" name=\"defint\"/>",
            "<OMS cd=\"arith1\" name=\"sum\"/>",
        ),
    ] {
        assert!(import(object(&body).as_bytes()).is_err(), "{body}");
    }
}

#[test]
fn matrix_operators_exchange_authored_structure_and_exact_results() {
    use serde_json::json;
    let matrix = json!(["List", ["List", 1, 2], ["List", 3, 4]]);
    for (expression, symbol) in [
        (json!(["Determinant", matrix]), "determinant"),
        (json!(["Transpose", matrix]), "transpose"),
        (json!(["Determinant", ["Transpose", matrix]]), "determinant"),
        (
            json!(["Transpose", ["List", ["List", 1, 2, 3], ["List", 4, 5, 6]]]),
            "transpose",
        ),
    ] {
        let c = from_mathjson(expression);
        let xml = String::from_utf8(export(c.project(), 1).unwrap()).unwrap();
        assert!(xml.contains(&format!("<OMS cd=\"linalg1\" name=\"{symbol}\"/>")));
        assert!(roundtrip(&c).snapshot().cells[0].cell.output.is_some());
        assert!(import(xml.replace("cd=\"linalg1\"", "cd=\"foreign\"").as_bytes()).is_err());
    }
    let c = roundtrip(&from_mathjson(json!(["Determinant", matrix])));
    assert_eq!(c.snapshot().cells[0].result_text.as_deref(), Some("-2"));
    let bad = r#"<OMA><OMS cd="linalg1" name="transpose"/><OMI>5</OMI></OMA>"#;
    assert!(import(object(bad).as_bytes()).is_err());
    assert!(import(object(&bad.replace("transpose", "inverse")).as_bytes()).is_err());
    let bad = r#"<OMA><OMS cd="linalg1" name="determinant"/><OMA><OMS cd="linalg2" name="matrix"/><OMA><OMS cd="linalg2" name="matrixrow"/><OMI>1</OMI><OMI>2</OMI></OMA></OMA></OMA>"#;
    assert!(import(object(bad).as_bytes()).is_err());
}

#[test]
fn latex_reference_matrix_definitions_export_without_mutating_the_notebook() {
    let mut c = AppCore::new(Project::empty("Matrices").unwrap()).unwrap();
    edit(
        &mut c,
        Command::DefineLatex {
            id: None,
            name: "a".into(),
            source: r"\begin{bmatrix}1&2&3\\4&5&6\end{bmatrix}".into(),
        },
    );
    edit(
        &mut c,
        Command::DefineLatex {
            id: None,
            name: "t".into(),
            source: r"\left(\cell{a}\right)^{T}".into(),
        },
    );
    let before = serde_json::to_value(c.snapshot()).unwrap();
    let copy = import(&export(c.project(), 2).unwrap()).unwrap();
    assert_eq!(
        c.snapshot().cells[1].result_text,
        copy.snapshot().cells[0].result_text
    );
    assert!(copy.snapshot().dependencies.is_empty());
    assert_eq!(before, serde_json::to_value(c.snapshot()).unwrap());
    assert_eq!(copy.snapshot().cells[0].kind, "matrix");
}
