use sha2::{Digest, Sha256};
use symbio_core::{reproduction::*, *};
fn edit(c: &mut AppCore, command: Command) {
    c.apply(Request {
        expected_revision: c.project().revision,
        actor: Actor::Human,
        command,
    })
    .unwrap();
}
fn define(c: &mut AppCore, id: Option<u64>, name: &str, source: &str) {
    edit(
        c,
        Command::Define {
            id,
            name: name.into(),
            source: source.into(),
        },
    );
}
fn core() -> AppCore {
    AppCore::new(Project::empty("Replay study").unwrap()).unwrap()
}
fn seal(p: &mut Package) {
    p.sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&p.payload).unwrap())
    );
}
#[test]
fn source_replay_preserves_references_exact_values_and_owner_state() {
    let mut c = core();
    define(&mut c, None, "a", "x^2");
    define(&mut c, None, "b", "@a+1/3");
    edit(
        &mut c,
        Command::Substitute {
            id: 2,
            values: std::collections::BTreeMap::from([("x".into(), "0.1".into())]),
        },
    );
    edit(
        &mut c,
        Command::Branch {
            name: "trial".into(),
        },
    );
    let before = serde_json::to_value(c.snapshot()).unwrap();
    let p = capture(c.project()).unwrap();
    assert_eq!(p.payload.branch, "trial");
    assert_eq!(p.payload.cells.len(), 2);
    let report = verify(&from_bytes(&to_bytes(&p).unwrap()).unwrap()).unwrap();
    assert!(report.reproduced);
    assert_eq!(report.matched, 2);
    assert_eq!(before, serde_json::to_value(c.snapshot()).unwrap());
    let mut reversed = p.clone();
    reversed.payload.cells.reverse();
    seal(&mut reversed);
    assert!(verify(&reversed).unwrap().reproduced);
}
#[test]
fn checksum_rejects_edits_and_resealed_changed_source_is_recomputed() {
    let mut c = core();
    define(&mut c, None, "a", "x+1");
    let mut p = capture(c.project()).unwrap();
    p.payload.cells[0].source = "x+2".into();
    assert!(from_bytes(&to_bytes(&p).unwrap()).is_err());
    seal(&mut p);
    let r = verify(&p).unwrap();
    assert!(!r.reproduced);
    assert_eq!(r.mismatched, 1);
    assert!(r.cells[0].differences.contains(&"expression"));
    assert!(r.cells[0].result_mathml.as_ref().unwrap().contains("2"));
}
#[test]
fn numeric_bits_conditions_and_engine_differences_are_reported() {
    let mut c = core();
    define(&mut c, None, "model", "x^2-y^2");
    edit(
        &mut c,
        Command::Transform {
            id: 1,
            operation: "evaluate".into(),
            variable: "x".into(),
            bindings: std::collections::BTreeMap::from([("x".into(), 2.), ("y".into(), 3.)]),
        },
    );
    let p = capture(c.project()).unwrap();
    assert!(verify(&p).unwrap().reproduced);
    assert_eq!(
        p.payload.cells[0]
            .expected
            .output
            .as_ref()
            .unwrap()
            .numeric_bits,
        Some(format!("{:016x}", (-5f64).to_bits()))
    );
    let mut altered = p.clone();
    altered.payload.cells[0]
        .expected
        .output
        .as_mut()
        .unwrap()
        .numeric_bits = Some("0000000000000000".into());
    seal(&mut altered);
    assert!(
        verify(&altered).unwrap().cells[0]
            .differences
            .contains(&"numeric_value")
    );
    let mut altered = p.clone();
    altered.payload.cells[0]
        .expected
        .output
        .as_mut()
        .unwrap()
        .conditions
        .clear();
    seal(&mut altered);
    assert!(
        verify(&altered).unwrap().cells[0]
            .differences
            .contains(&"evidence")
    );
    let mut altered = p;
    altered.payload.engine = "older-engine".into();
    seal(&mut altered);
    let r = verify(&altered).unwrap();
    assert!(!r.reproduced);
    assert!(!r.engine_matches);
}
#[test]
fn errors_are_reproduced_separately_from_successful_results() {
    let mut c = core();
    define(&mut c, None, "a", "x+1");
    edit(
        &mut c,
        Command::Substitute {
            id: 1,
            values: std::collections::BTreeMap::from([("x".into(), "2".into())]),
        },
    );
    define(&mut c, Some(1), "a", "y+1");
    let r = verify(&capture(c.project()).unwrap()).unwrap();
    assert!(r.reproduced);
    assert_eq!(r.errors_reproduced, 1);
    assert_eq!(r.matched, 0);
    assert_eq!(r.cells[0].status, "error_reproduced");
}
#[test]
fn dimension_failures_are_replayed_without_trusting_recorded_error() {
    let mut c = core();
    define(&mut c, None, "a", "x+1");
    edit(
        &mut c,
        Command::Assume {
            symbol: "x".into(),
            assumption: Assumption {
                dimension: Some([1, 0, 0]),
                ..Default::default()
            },
        },
    );
    let mut p = capture(c.project()).unwrap();
    let r = verify(&p).unwrap();
    assert_eq!(r.errors_reproduced, 1);
    p.payload.cells[0].expected.error = Some("unsupported".into());
    seal(&mut p);
    assert_eq!(verify(&p).unwrap().mismatched, 1);
}
#[test]
fn missing_recipes_results_and_empty_packages_are_not_claimed_reproduced() {
    let mut c = core();
    assert!(!verify(&capture(c.project()).unwrap()).unwrap().reproduced);
    define(&mut c, None, "a", "2");
    let mut p = capture(c.project()).unwrap();
    p.payload.cells[0].calculation = None;
    seal(&mut p);
    let r = verify(&p).unwrap();
    assert!(!r.reproduced);
    assert_eq!(r.skipped, 1);
    let mut p = capture(c.project()).unwrap();
    p.payload.cells[0].expected.output = None;
    seal(&mut p);
    assert_eq!(verify(&p).unwrap().skipped, 1);
}
#[test]
fn unknown_fields_versions_limits_and_invalid_graph_references_are_rejected() {
    let mut c = core();
    define(&mut c, None, "a", "x");
    let p = capture(c.project()).unwrap();
    let mut v = serde_json::to_value(&p).unwrap();
    v["shell_command"] = "echo bad".into();
    assert!(from_bytes(&serde_json::to_vec(&v).unwrap()).is_err());
    let mut bad = p.clone();
    bad.version = 2;
    assert!(verify(&bad).is_err());
    let mut bad = p.clone();
    bad.payload.cells[0].source = "@missing".into();
    seal(&mut bad);
    assert!(verify(&bad).is_err());
    let mut bad = p.clone();
    bad.payload.cells[0].source = "@a".into();
    seal(&mut bad);
    assert!(verify(&bad).is_err());
    let mut bad = p;
    bad.payload.cells[0].source = "x".repeat(2049);
    seal(&mut bad);
    assert!(verify(&bad).is_err());
    assert!(from_bytes(&vec![b' '; MAX_BYTES + 1]).is_err());
}
#[test]
fn structural_hash_is_independent_of_collection_and_reloading() {
    let mut c = core();
    define(&mut c, None, "a", "(x+1)^2");
    let before = capture(c.project()).unwrap().payload.cells[0]
        .expected
        .clone();
    edit(&mut c, Command::Compact {});
    let after = capture(c.project()).unwrap();
    assert_eq!(before, after.payload.cells[0].expected);
    assert!(verify(&after).unwrap().reproduced);
    let restored =
        AppCore::new(serde_json::from_slice(&serde_json::to_vec(c.project()).unwrap()).unwrap())
            .unwrap();
    assert!(
        verify(&capture(restored.project()).unwrap())
            .unwrap()
            .reproduced
    );
}
#[test]
fn package_io_is_bounded_and_never_overwrites_existing_files() {
    let mut c = core();
    define(&mut c, None, "a", "2");
    let p = capture(c.project()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.json");
    write_new(&path, &p).unwrap();
    assert!(verify(&read(&path).unwrap()).unwrap().reproduced);
    let before = std::fs::read(&path).unwrap();
    assert!(write_new(&path, &p).is_err());
    assert_eq!(before, std::fs::read(&path).unwrap());
    #[cfg(unix)]
    {
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(write_new(&link, &p).is_err());
        assert_eq!(before, std::fs::read(&path).unwrap());
    }
}

#[test]
fn early_sample_domain_annotations_replay_without_editing_the_document() {
    let c = AppCore::new(Project::new("Sample").unwrap()).unwrap();
    let before = serde_json::to_vec(c.project()).unwrap();
    let p = capture(c.project()).unwrap();
    assert!(verify(&p).unwrap().reproduced);
    assert_eq!(before, serde_json::to_vec(c.project()).unwrap());
}

#[test]
fn latex_integrals_matrices_and_numeric_equations_replay_from_source() {
    let mut c = core();
    for (name, source) in [
        ("integral", r"\int_0^1 x^2\,dx"),
        ("matrix", r"\begin{pmatrix}1&2\\3&4\end{pmatrix}"),
        ("model", r"z=x^2-y^2"),
    ] {
        edit(
            &mut c,
            Command::DefineLatex {
                id: None,
                name: name.into(),
                source: source.into(),
            },
        );
    }
    edit(
        &mut c,
        Command::Transform {
            id: 2,
            operation: "determinant".into(),
            variable: "x".into(),
            bindings: Default::default(),
        },
    );
    edit(
        &mut c,
        Command::Transform {
            id: 3,
            operation: "evaluate".into(),
            variable: "x".into(),
            bindings: std::collections::BTreeMap::from([("x".into(), 2.), ("y".into(), 3.)]),
        },
    );
    let p = capture(c.project()).unwrap();
    let r = verify(&p).unwrap();
    assert!(r.reproduced);
    assert_eq!(r.matched, 3);
    let mut old = p;
    old.payload.cells[0]
        .expected
        .output
        .as_mut()
        .unwrap()
        .engine = "symbio-rust/0.4.0".into();
    seal(&mut old);
    let r = verify(&old).unwrap();
    assert!(r.engine_matches);
    assert!(!r.reproduced);
    assert!(r.cells[0].differences.contains(&"engine"));
    assert_eq!(
        r.cells[0].expected.output.as_ref().unwrap().engine,
        "symbio-rust/0.4.0"
    );
}

#[test]
fn shared_dag_results_are_checked_without_exponential_preview_expansion() {
    let payload = Payload {
        project_id: uuid::Uuid::new_v4(),
        document: "DAG".into(),
        branch: "main".into(),
        revision: 0,
        engine: ENGINE.into(),
        assumptions: Default::default(),
        cells: (1..=12)
            .map(|id| RecipeCell {
                id,
                name: format!("a{id}"),
                source: if id == 1 {
                    "x+y".into()
                } else {
                    format!("@a{}+@a{}", id - 1, id - 1)
                },
                source_format: SourceFormat::Expression,
                note: String::new(),
                calculation: Some(Calculation::default()),
                expected: Expected {
                    error: None,
                    output: Some(Evidence {
                        expression_sha256: Some("0".repeat(64)),
                        numeric_bits: None,
                        operation: "compute".into(),
                        trust: Trust::Symbolic,
                        precision: "exact".into(),
                        conditions: vec![],
                        engine: ENGINE.into(),
                    }),
                },
            })
            .collect(),
    };
    let mut p = Package {
        format: "symbio.reproduction".into(),
        version: 1,
        payload,
        sha256: String::new(),
    };
    seal(&mut p);
    let r = verify(&p).unwrap();
    assert_eq!(r.mismatched, 12);
    assert!(r.cells.last().unwrap().result_preview_omitted);
    assert!(r.cells.last().unwrap().result_mathml.is_none());
    assert!(r.cells.first().unwrap().result_mathml.is_some());
}
