use symbio_core::*;
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
fn compare(c: &AppCore) -> comparison::BranchComparison {
    comparison::compare(c.project(), "main", "trial").unwrap()
}
fn fork(c: &mut AppCore) {
    edit(
        c,
        Command::Branch {
            name: "trial".into(),
        },
    );
}
#[test]
fn comparison_is_read_only_and_tracks_identity_add_remove_rename() {
    let mut c = AppCore::new(Project::empty("test").unwrap()).unwrap();
    define(&mut c, None, "a", "x+1");
    define(&mut c, None, "b", "2");
    fork(&mut c);
    define(&mut c, Some(1), "renamed", "x+1");
    edit(&mut c, Command::Remove { id: 2 });
    define(&mut c, None, "new", "3");
    let before = serde_json::to_value(c.project()).unwrap();
    let dirty = c.dirty();
    let d = compare(&c);
    assert_eq!(d.cells.len(), 3);
    assert_eq!(d.cells[0].fields, vec!["name"]);
    assert_eq!(d.cells[1].status, "removed");
    assert_eq!(d.cells[2].status, "added");
    assert_eq!(before, serde_json::to_value(c.project()).unwrap());
    assert_eq!(dirty, c.dirty());
    assert_eq!(c.project().active, "trial");
    assert!(comparison::compare(c.project(), "missing", "trial").is_err());
    let same = comparison::compare(c.project(), "trial", "trial").unwrap();
    assert!(same.cells.is_empty());
    assert_eq!(same.unchanged_cells, 2);
}
#[test]
fn changes_in_reference_dependencies_and_recipes_are_visible() {
    let mut c = AppCore::new(Project::empty("test").unwrap()).unwrap();
    define(&mut c, None, "a", "2");
    define(&mut c, None, "b", "@a+x");
    edit(
        &mut c,
        Command::Substitute {
            id: 2,
            values: std::collections::BTreeMap::from([("x".into(), "1/3".into())]),
        },
    );
    fork(&mut c);
    define(&mut c, Some(1), "a", "4");
    let d = compare(&c);
    let b = d.cells.iter().find(|c| c.id == 2).unwrap();
    assert!(b.fields.contains(&"resolved"));
    assert!(b.fields.contains(&"result"));
    assert!(!b.fields.contains(&"definition"));
    assert!(!b.fields.contains(&"calculation"));
    edit(
        &mut c,
        Command::Substitute {
            id: 2,
            values: std::collections::BTreeMap::from([("x".into(), "2".into())]),
        },
    );
    let d = compare(&c);
    assert!(
        d.cells
            .iter()
            .find(|c| c.id == 2)
            .unwrap()
            .fields
            .contains(&"calculation")
    );
}
#[test]
fn assumptions_evidence_notes_and_source_notation_are_distinguished() {
    let mut c = AppCore::new(Project::empty("test").unwrap()).unwrap();
    define(&mut c, None, "a", "sqrt(x^2)");
    edit(
        &mut c,
        Command::Transform {
            id: 1,
            operation: "compute".into(),
            variable: "x".into(),
            bindings: Default::default(),
        },
    );
    fork(&mut c);
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
        Command::Note {
            id: 1,
            text: "real domain trial".into(),
        },
    );
    let d = compare(&c);
    assert_eq!(d.assumptions.len(), 1);
    assert!(d.assumptions[0].base.is_none());
    assert_eq!(
        d.assumptions[0].target.as_ref().unwrap().domain,
        Domain::Real
    );
    assert!(d.cells[0].fields.contains(&"result"));
    assert!(d.cells[0].fields.contains(&"note"));
    assert!(!d.cells[0].fields.contains(&"definition"));
    edit(
        &mut c,
        Command::DefineLatex {
            id: Some(1),
            name: "a".into(),
            source: "\\sqrt{x^2}".into(),
        },
    );
    let d = compare(&c);
    assert!(d.cells[0].fields.contains(&"source"));
    assert!(!d.cells[0].fields.contains(&"definition"));
}
#[test]
fn comparison_survives_collection_and_reload_and_does_not_claim_equivalence() {
    let mut c = AppCore::new(Project::empty("test").unwrap()).unwrap();
    define(&mut c, None, "a", "x+x");
    fork(&mut c);
    define(&mut c, Some(1), "a", "2*x");
    let before = serde_json::to_value(compare(&c)).unwrap();
    edit(&mut c, Command::Compact {});
    let mut after = serde_json::to_value(compare(&c)).unwrap();
    after["revision"] = before["revision"].clone();
    // IDs inside evidence may be rewritten by collection, but rendered comparison stays stable.
    assert_eq!(before, after);
    assert!(compare(&c).cells[0].fields.contains(&"definition"));
    let p: Project = serde_json::from_slice(&serde_json::to_vec(c.project()).unwrap()).unwrap();
    let restored = AppCore::new(p).unwrap();
    assert_eq!(
        serde_json::to_value(compare(&c)).unwrap(),
        serde_json::to_value(compare(&restored)).unwrap()
    );
}

#[test]
fn evidence_only_changes_and_errors_are_not_hidden_by_equal_formulas() {
    let mut c = AppCore::new(Project::empty("test").unwrap()).unwrap();
    define(&mut c, None, "a", "x/x");
    edit(
        &mut c,
        Command::Assume {
            symbol: "x".into(),
            assumption: Assumption {
                nonzero: true,
                ..Default::default()
            },
        },
    );
    edit(
        &mut c,
        Command::Transform {
            id: 1,
            operation: "compute".into(),
            variable: "x".into(),
            bindings: Default::default(),
        },
    );
    fork(&mut c);
    edit(
        &mut c,
        Command::Assume {
            symbol: "x".into(),
            assumption: Assumption {
                domain: Domain::Real,
                nonzero: true,
                ..Default::default()
            },
        },
    );
    let d = compare(&c);
    assert!(d.cells[0].fields.contains(&"result"));
    assert_eq!(
        d.cells[0].base.as_ref().unwrap().result_mathml,
        d.cells[0].target.as_ref().unwrap().result_mathml
    );
    // The value is equal, but the recorded domain conditions differ.
    let reverse = comparison::compare(c.project(), "trial", "main").unwrap();
    assert_eq!(
        reverse.assumptions[0].base.as_ref().unwrap().domain,
        Domain::Real
    );
    define(&mut c, Some(1), "a", "x+1");
    edit(
        &mut c,
        Command::Substitute {
            id: 1,
            values: std::collections::BTreeMap::from([("x".into(), "2".into())]),
        },
    );
    define(&mut c, Some(1), "a", "y+1");
    let d = compare(&c);
    let row = &d.cells[0];
    assert!(row.fields.contains(&"calculation_error"));
    assert!(row.target.as_ref().unwrap().calculation_error.is_some());
}
