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
        r#"<OMA><OMI>2</OMI><OMI>1</OMI></OMA>"#,
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
fn atomic_no_clobber_is_explicit() {
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

#[test]
fn column_oriented_matrices_preserve_rectangular_orientation() {
    let body = r#"<OMA><OMS cd="linalg3" name="matrix"/><OMA><OMS cd="linalg3" name="matrixcolumn"/><OMI>1</OMI><OMI>4</OMI></OMA><OMA><OMS cd="linalg3" name="matrixcolumn"/><OMI>2</OMI><OMI>5</OMI></OMA><OMA><OMS cd="linalg3" name="matrixcolumn"/><OMI>3</OMI><OMI>6</OMI></OMA></OMA>"#;
    let c = load(body);
    let expected = from_mathjson(serde_json::json!([
        "List",
        ["List", 1, 2, 3],
        ["List", 4, 5, 6]
    ]));
    assert_eq!(
        c.snapshot().cells[0].result_text,
        expected.snapshot().cells[0].result_text
    );
    let xml = String::from_utf8(export(c.project(), 1).unwrap()).unwrap();
    assert!(xml.contains("matrixrow"));
    assert_eq!(
        roundtrip(&c).snapshot().cells[0].result_text,
        expected.snapshot().cells[0].result_text
    );
    let transposed = load(&format!(
        r#"<OMA><OMS cd="linalg1" name="transpose"/>{body}</OMA>"#
    ));
    let expected = from_mathjson(serde_json::json!([
        "List",
        ["List", 1, 4],
        ["List", 2, 5],
        ["List", 3, 6]
    ]));
    assert_eq!(
        transposed.snapshot().cells[0].result_text,
        expected.snapshot().cells[0].result_text
    );
    let square = r#"<OMA><OMS cd="linalg3" name="matrix"/><OMA><OMS cd="linalg3" name="matrixcolumn"/><OMI>1</OMI><OMI>3</OMI></OMA><OMA><OMS cd="linalg3" name="matrixcolumn"/><OMI>2</OMI><OMI>4</OMI></OMA></OMA>"#;
    let determinant = load(&format!(
        r#"<OMA><OMS cd="linalg1" name="determinant"/>{square}</OMA>"#
    ));
    assert_eq!(
        determinant.snapshot().cells[0].result_text.as_deref(),
        Some("-2")
    );
    for bad in [
        body.replace("matrixcolumn", "matrixrow"),
        body.replacen("<OMI>4</OMI>", "", 1),
        body.replace(r#"cd="linalg3""#, r#"cd="foreign""#),
        r#"<OMA><OMS cd="linalg3" name="matrix"/></OMA>"#.to_owned(),
        r#"<OMA><OMS cd="linalg3" name="matrix"/><OMA><OMS cd="linalg3" name="matrixcolumn"/></OMA></OMA>"#.to_owned(),
    ] {
        assert!(import(object(&bad).as_bytes()).is_err(), "{bad}");
    }
}

#[test]
fn matrix_products_exchange_in_operand_order() {
    use serde_json::json;
    let left = json!(["List", ["List", 1, 2]]);
    let right = json!(["List", ["List", 3], ["List", 4]]);
    for (a, b, expected) in [
        (&left, &right, json!(["List", ["List", 11]])),
        (
            &right,
            &left,
            json!(["List", ["List", 3, 6], ["List", 4, 8]]),
        ),
    ] {
        let c = from_mathjson(json!(["MatrixMultiply", a, b]));
        let xml = String::from_utf8(export(c.project(), 1).unwrap()).unwrap();
        assert!(xml.contains(r#"<OMS cd="arith1" name="times"/>"#));
        let copy = import(xml.as_bytes()).unwrap();
        assert_eq!(
            copy.snapshot().cells[0].result_text,
            from_mathjson(expected).snapshot().cells[0].result_text
        );
        assert_eq!(
            symbio_core::mathjson::export(copy.project(), 1)
                .unwrap()
                .expression[0],
            "MatrixMultiply"
        );
    }

    let row = r#"<OMA><OMS cd="linalg2" name="matrix"/><OMA><OMS cd="linalg2" name="matrixrow"/><OMI>1</OMI><OMI>2</OMI></OMA></OMA>"#;
    let invalid = format!(r#"<OMA><OMS cd="arith1" name="times"/>{row}{row}</OMA>"#);
    assert!(import(object(&invalid).as_bytes()).is_err());
}

#[test]
fn matrix_sums_and_differences_roundtrip_with_exact_entries() {
    use serde_json::json;

    let left = json!(["List", ["List", ["Rational", 1, 2], 5]]);
    let right = json!(["List", ["List", ["Rational", 1, 3], 8]]);
    for (head, symbol, fraction, second) in [("Add", "plus", 5, 13), ("Subtract", "minus", 1, -3)] {
        let source = from_mathjson(json!([head, left, right]));
        let xml = String::from_utf8(export(source.project(), 1).unwrap()).unwrap();
        let operator = format!(r#"<OMS cd="arith1" name="{symbol}"/>"#);
        assert!(xml.contains(&operator));
        let imported = import(xml.as_bytes()).unwrap();
        let expected = from_mathjson(json!(["List", ["List", ["Rational", fraction, 6], second]]));
        assert_eq!(
            imported.snapshot().cells[0].result_text,
            expected.snapshot().cells[0].result_text
        );
        assert_eq!(
            symbio_core::mathjson::export(imported.project(), 1)
                .unwrap()
                .expression[0],
            head
        );

        let matrix = r#"<OMA><OMS cd="linalg2" name="matrix"/><OMA><OMS cd="linalg2" name="matrixrow"/><OMI>1</OMI></OMA></OMA>"#;
        let wide = matrix.replace("<OMI>1</OMI>", "<OMI>1</OMI><OMI>2</OMI>");
        for invalid in [wide.as_str(), "<OMI>1</OMI>"] {
            assert!(
                import(object(&format!("<OMA>{operator}{matrix}{invalid}</OMA>")).as_bytes())
                    .is_err()
            );
        }
    }
}

#[test]
fn inverse_roundtrip_preserves_exact_result_and_singular_error() {
    use serde_json::json;
    for (matrix, singular) in [
        (json!(["List", ["List", 2, 1], ["List", 0, 3]]), false),
        (json!(["List", ["List", 1, 2], ["List", 2, 4]]), true),
    ] {
        let source = from_mathjson(json!(["Inverse", matrix]));
        let expected = symbio_core::mathjson::export(source.project(), 1)
            .unwrap()
            .expression;
        let xml = String::from_utf8(export(source.project(), 1).unwrap()).unwrap();
        assert!(xml.contains(r#"<OMS cd="arith1" name="power"/>"#));
        assert!(xml.contains("<OMI>-1</OMI>"));
        let imported = import(xml.as_bytes()).unwrap();
        let exchanged = symbio_core::mathjson::export(imported.project(), 1).unwrap();
        assert_eq!(exchanged.expression, expected);
        let returned =
            symbio_core::mathjson::import(&symbio_core::mathjson::to_bytes(&exchanged).unwrap())
                .unwrap();
        for core in [&imported, &returned] {
            let snapshot = core.snapshot();
            let cell = &snapshot.cells[0];
            if singular {
                assert_eq!(cell.cell.calculation_error.as_deref(), Some("domain_error"));
                assert!(cell.result_text.is_none());
            } else {
                let exact = from_mathjson(json!([
                    "List",
                    ["List", ["Rational", 1, 2], ["Rational", -1, 6]],
                    ["List", 0, ["Rational", 1, 3]]
                ]));
                assert_eq!(cell.result_text, exact.snapshot().cells[0].result_text);
            }
        }
    }
}

#[test]
fn matrix_powers_exchange_authored_exponents_and_exact_results() {
    use serde_json::json;
    let matrix = json!(["List", ["List", 2, 0], ["List", 0, 3]]);
    for (exponent, expected) in [
        (0, "matrix(2x2; 1, 0, 0, 1)"),
        (2, "matrix(2x2; 4, 0, 0, 9)"),
        (-2, "matrix(2x2; 1/4, 0, 0, 1/9)"),
    ] {
        let original = from_mathjson(json!(["Power", matrix, exponent]));
        let copy = roundtrip(&original);
        assert_eq!(
            copy.snapshot().cells[0].result_text.as_deref(),
            Some(expected)
        );
        let authored = symbio_core::mathjson::export(original.project(), 1).unwrap();
        let exchanged = symbio_core::mathjson::export(copy.project(), 1).unwrap();
        assert_eq!(authored.expression, exchanged.expression);
        assert_eq!(exchanged.expression[0], "Power");
    }

    let singular = json!(["List", ["List", 1, 2], ["List", 2, 4]]);
    let copy = roundtrip(&from_mathjson(json!(["Power", singular, -2])));
    assert_eq!(
        copy.snapshot().cells[0].cell.calculation_error.as_deref(),
        Some("domain_error")
    );
}

#[test]
fn matrix_scalar_division_roundtrips_exact_values_and_zero_error() {
    use serde_json::json;
    let matrix = json!(["List", ["List", 1, 2], ["List", 3, 4]]);
    for denominator in [3, 0] {
        let source = from_mathjson(json!(["Divide", matrix, denominator]));
        let xml = String::from_utf8(export(source.project(), 1).unwrap()).unwrap();
        assert!(xml.contains(r#"<OMS cd="arith1" name="divide"/>"#));
        let imported = roundtrip(&source);
        let exchange = symbio_core::mathjson::export(imported.project(), 1).unwrap();
        assert_eq!(exchange.expression[0], "Divide");
        let copy =
            symbio_core::mathjson::import(&symbio_core::mathjson::to_bytes(&exchange).unwrap())
                .unwrap();
        for core in [&imported, &copy] {
            let snapshot = core.snapshot();
            let cell = &snapshot.cells[0];
            if denominator == 0 {
                assert_eq!(cell.cell.calculation_error.as_deref(), Some("domain_error"));
                assert!(cell.result_text.is_none());
            } else {
                assert_eq!(
                    cell.result_text.as_deref(),
                    Some("matrix(2x2; 1/3, 2/3, 1, 4/3)")
                );
            }
        }
    }
    let matrix = r#"<OMA><OMS cd="linalg2" name="matrix"/><OMA><OMS cd="linalg2" name="matrixrow"/><OMI>2</OMI></OMA></OMA>"#;
    let invalid = format!(r#"<OMA><OMS cd="arith1" name="divide"/>{matrix}{matrix}</OMA>"#);
    assert!(import(object(&invalid).as_bytes()).is_err());
}

#[test]
fn named_applications_preserve_argument_order_and_nesting() {
    use serde_json::json;
    let body = r#"<OMA><OMV name="outer_2"/><OMV name="y"/><OMA><OMV name="inner"/><OMI>2</OMI><OMV name="x"/></OMA><OMI>1</OMI><OMA><OMV name="constant_fn"/></OMA></OMA>"#;
    let core = load(body);
    let expected = json!(["Apply", {"sym":"outer_2"}, "y",
        ["Apply", {"sym":"inner"}, {"num":"2"}, "x"], {"num":"1"},
        ["Apply", {"sym":"constant_fn"}]]);
    let exchange = symbio_core::mathjson::export(core.project(), 1).unwrap();
    assert_eq!(exchange.expression, expected);
    let imported =
        symbio_core::mathjson::import(&symbio_core::mathjson::to_bytes(&exchange).unwrap())
            .unwrap();
    let xml = String::from_utf8(export(imported.project(), 1).unwrap()).unwrap();
    assert!(xml.contains(body));
    let copy = roundtrip(&imported);
    assert_eq!(
        copy.snapshot().cells[0].result_text.as_deref(),
        Some("outer_2(y, inner(2, x), 1, constant_fn())")
    );
    assert!(import(object(&body.replace("outer_2", "bad-name")).as_bytes()).is_err());
}

#[test]
fn matrix_selector_reports_domain_error_for_nonpositive_or_fractional_indices() {
    use serde_json::json;
    for index in [json!(0), json!(-1), json!(["Rational", 1, 2])] {
        let app = from_mathjson(json!(["MatrixSelector", index, 1, ["List", ["List", 7]]]));
        let xml = export(app.project(), 1).unwrap();
        let copy = import(&xml).unwrap();
        let snapshot = copy.snapshot();
        assert_eq!(
            snapshot.cells[0].cell.calculation_error.as_deref(),
            Some("domain_error")
        );
        assert!(snapshot.cells[0].result_text.is_none());
    }
}

#[test]
fn identity_rejects_invalid_orders_in_both_formats() {
    use serde_json::json;

    for (xml, order) in [
        ("<OMI>0</OMI>", json!(0)),
        ("<OMI>-1</OMI>", json!(-1)),
        ("<OMI>9</OMI>", json!(9)),
        (
            r#"<OMA><OMS cd="nums1" name="rational"/><OMI>3</OMI><OMI>2</OMI></OMA>"#,
            json!(["Rational", 3, 2]),
        ),
    ] {
        let body = format!(r#"<OMA><OMS cd="linalg5" name="identity"/>{xml}</OMA>"#);
        assert!(matches!(
            import(object(&body).as_bytes()),
            Err(Error::Domain(_))
        ));
        let input = serde_json::to_vec(&json!(["IdentityMatrix", order])).unwrap();
        assert!(matches!(
            symbio_core::mathjson::import(&input),
            Err(Error::Domain(_))
        ));
    }
}

#[test]
fn vector_selector_accepts_columns_and_matrix_rows() {
    for (cd, constructor) in [("linalg3", "vector"), ("linalg2", "matrixrow")] {
        let body = format!(
            r#"<OMA><OMS cd="linalg1" name="vector_selector"/><OMI>2</OMI><OMA><OMS cd="{cd}" name="{constructor}"/><OMI>7</OMI><OMA><OMS cd="nums1" name="rational"/><OMI>2</OMI><OMI>3</OMI></OMA></OMA></OMA>"#
        );
        let source = load(&body);
        let copy = roundtrip(&source);
        assert_eq!(copy.snapshot().cells[0].result_text.as_deref(), Some("2/3"));
        let xml = String::from_utf8(export(copy.project(), 1).unwrap()).unwrap();
        assert!(xml.contains(r#"<OMS cd="linalg2" name="vector"/>"#));
        let outside = load(&body.replacen("<OMI>2</OMI>", "<OMI>3</OMI>", 1));
        assert_eq!(
            outside.snapshot().cells[0]
                .cell
                .calculation_error
                .as_deref(),
            Some("domain_error")
        );
    }
}

#[test]
fn scalar_product_accepts_column_vectors_and_preserves_operand_order() {
    use serde_json::json;

    let body = r#"<OMA><OMS cd="linalg1" name="scalarproduct"/>
      <OMA><OMS cd="linalg3" name="vector"/><OMI>1</OMI><OMA><OMS cd="nums1" name="rational"/><OMI>2</OMI><OMI>3</OMI></OMA></OMA>
      <OMA><OMS cd="linalg3" name="vector"/><OMI>2</OMI><OMI>4</OMI></OMA></OMA>"#;
    let app = load(body);
    let copy = roundtrip(&app);
    assert_eq!(
        copy.snapshot().cells[0].result_text.as_deref(),
        Some("14/3")
    );
    let expression = symbio_core::mathjson::export(copy.project(), 1)
        .unwrap()
        .expression;
    assert_eq!(
        expression,
        json!(["ScalarProduct",
            ["Vector", {"num":"1"}, ["Divide", {"num":"2"}, {"num":"3"}]],
            ["Vector", {"num":"2"}, {"num":"4"}]
        ])
    );
    let mismatched = load(&body.replace("<OMI>4</OMI>", ""));
    assert_eq!(
        mismatched.snapshot().cells[0]
            .cell
            .calculation_error
            .as_deref(),
        Some("domain_error")
    );
}

#[test]
fn cross_and_outer_products_accept_column_vector_operands() {
    let column = r#"<OMA><OMS cd="linalg3" name="vector"/><OMA><OMS cd="nums1" name="rational"/><OMI>1</OMI><OMI>2</OMI></OMA><OMI>2</OMI><OMI>3</OMI></OMA>"#;
    let row = r#"<OMA><OMS cd="linalg2" name="vector"/><OMI>4</OMI><OMI>5</OMI><OMI>6</OMI></OMA>"#;
    let short = row.replace("<OMI>6</OMI>", "");
    for (operator, head, right, result) in [
        (
            "vectorproduct",
            "VectorProduct",
            row,
            "matrix(1x3; -3, 9, -11/2)",
        ),
        (
            "outerproduct",
            "OuterProduct",
            short.as_str(),
            "matrix(3x2; 2, 5/2, 8, 10, 12, 15)",
        ),
    ] {
        let body = format!(r#"<OMA><OMS cd="linalg1" name="{operator}"/>{column}{right}</OMA>"#);
        let copy = roundtrip(&load(&body));
        assert_eq!(
            copy.snapshot().cells[0].result_text.as_deref(),
            Some(result)
        );
        let mathjson = symbio_core::mathjson::export(copy.project(), 1)
            .unwrap()
            .expression;
        assert_eq!(mathjson[0], head);
        assert_eq!(mathjson[1][1][0], "Divide");
        assert_eq!(mathjson[2][1]["num"], "4");
    }
}

#[test]
fn linalg2col_matrix_and_vector_imports_roundtrip() {
    let column = |top, bottom| {
        format!(
            r#"<OMA><OMS cd="linalg2col" name="matrixcolumn"/><OMI>{top}</OMI><OMI>{bottom}</OMI></OMA>"#
        )
    };
    let matrix = format!(
        r#"<OMA><OMS cd="linalg2col" name="matrix"/>{}{}{}</OMA>"#,
        column(1, 4),
        column(2, 5),
        column(3, 6)
    );
    let body = format!(
        r#"<OMA><OMS cd="linalg1" name="matrix_selector"/><OMI>2</OMI><OMI>3</OMI>{matrix}</OMA>"#
    );
    assert_eq!(
        roundtrip(&load(&body)).snapshot().cells[0]
            .result_text
            .as_deref(),
        Some("6")
    );
    let invalid = matrix.replacen(
        r#"cd="linalg2col" name="matrixcolumn""#,
        r#"cd="linalg3" name="matrixcolumn""#,
        1,
    );
    assert!(import(object(&invalid).as_bytes()).is_err());

    let vector = r#"<OMA><OMS cd="linalg2col" name="vector"/><OMI>2</OMI><OMI>3</OMI></OMA>"#;
    let product = format!(r#"<OMA><OMS cd="linalg1" name="scalarproduct"/>{vector}{vector}</OMA>"#);
    assert_eq!(
        roundtrip(&load(&product)).snapshot().cells[0]
            .result_text
            .as_deref(),
        Some("13")
    );
}

#[test]
fn vector_size_roundtrips_with_ordered_entries() {
    use serde_json::json;
    for cd in ["linalg2", "linalg3", "linalg2col"] {
        let body = format!(
            r#"<OMA><OMS cd="linalg4" name="size"/><OMA><OMS cd="{cd}" name="vector"/><OMI>7</OMI><OMV name="x"/><OMI>-2</OMI></OMA></OMA>"#
        );
        let copy = roundtrip(&load(&body));
        assert_eq!(copy.snapshot().cells[0].result_text.as_deref(), Some("3"));
        let encoded = symbio_core::mathjson::export(copy.project(), 1).unwrap();
        assert_eq!(
            encoded.expression,
            json!(["VectorSize", ["Vector", {"num":"7"}, "x", {"num":"-2"}]])
        );
    }
    assert!(
        import(object(r#"<OMA><OMS cd="linalg4" name="size"/><OMI>3</OMI></OMA>"#).as_bytes())
            .is_err()
    );
}

#[test]
fn nested_vector_tensor_preserves_grouping_and_operand_order() {
    use serde_json::json;
    let app = from_mathjson(json!([
        "VectorTensor",
        ["Vector", 2],
        [
            "VectorTensor",
            ["Vector", ["Rational", 1, 2], 3],
            ["Vector", 4, 5]
        ]
    ]));
    let copy = roundtrip(&app);
    assert_eq!(
        copy.snapshot().cells[0].result_text.as_deref(),
        Some("matrix(1x4; 4, 5, 24, 30)")
    );
    let expression = symbio_core::mathjson::export(copy.project(), 1)
        .unwrap()
        .expression;
    assert_eq!(expression[1], json!(["Vector", {"num":"2"}]));
    assert_eq!(
        expression[2],
        json!(["VectorTensor",
            ["Vector", ["Divide", {"num":"1"}, {"num":"2"}], {"num":"3"}],
            ["Vector", {"num":"4"}, {"num":"5"}]
        ])
    );
}

#[test]
fn scalar_product_accepts_ordered_vector_tensor_operands() {
    use serde_json::json;
    let tensor = json!([
        "VectorTensor",
        ["Vector", ["Rational", 1, 2], 2],
        ["Vector", 3, 4]
    ]);
    let vector = json!(["Vector", 0, 1, 0, 0]);
    for (left, right, expected) in [
        (tensor.clone(), vector.clone(), Some("2")),
        (vector, tensor.clone(), Some("2")),
        (tensor.clone(), tensor.clone(), Some("425/4")),
        (tensor, json!(["Vector", 1, 2, 3, 4, 5, 6, 7, 8, 9]), None),
    ] {
        let source = from_mathjson(json!(["ScalarProduct", left, right]));
        let copy = roundtrip(&source);
        let snapshot = copy.snapshot();
        assert_eq!(snapshot.cells[0].result_text.as_deref(), expected);
        if expected.is_none() {
            assert_eq!(
                snapshot.cells[0].cell.calculation_error.as_deref(),
                Some("domain_error")
            );
        }
    }
}
