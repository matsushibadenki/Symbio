use std::collections::BTreeMap;
use symbio_core::{
    graph::{Graph, Node},
    *,
};
fn core() -> AppCore {
    let mut project = Project::new("Dependencies").unwrap();
    project.graph = Graph::default();
    project.current_mut().cells.clear();
    project.current_mut().assumptions.clear();
    project.current_mut().selected = None;
    project.next_cell = 1;
    AppCore::new(project).unwrap()
}
fn edit(core: &mut AppCore, command: Command) -> Snapshot {
    core.apply(Request {
        expected_revision: core.project().revision,
        actor: Actor::Human,
        command,
    })
    .unwrap()
}
fn define(core: &mut AppCore, id: Option<u64>, name: &str, source: &str, latex: bool) -> Snapshot {
    edit(
        core,
        if latex {
            Command::DefineLatex {
                id,
                name: name.into(),
                source: source.into(),
            }
        } else {
            Command::Define {
                id,
                name: name.into(),
                source: source.into(),
            }
        },
    )
}
fn cell(snapshot: &Snapshot, id: u64) -> &CellView {
    snapshot.cells.iter().find(|c| c.cell.id == id).unwrap()
}
fn transform(core: &mut AppCore, id: u64, op: &str, bindings: BTreeMap<String, f64>) -> Snapshot {
    edit(
        core,
        Command::Transform {
            id,
            operation: op.into(),
            variable: "x".into(),
            bindings,
        },
    )
}
#[test]
fn chains_and_diamonds_recalculate_only_affected_cells_with_saved_numeric_recipe() {
    let mut c = core();
    define(&mut c, None, "a", "2", false);
    define(&mut c, None, "b", "@a+3", false);
    define(&mut c, None, "c", "@a*@b", false);
    let before = define(&mut c, None, "unrelated", "77", false);
    transform(&mut c, 3, "evaluate", BTreeMap::new());
    let s = define(&mut c, Some(1), "a", "4", false);
    assert_eq!(cell(&s, 2).result_text.as_deref(), Some("7"));
    assert_eq!(cell(&s, 3).cell.output.as_ref().unwrap().value, Some(28.0));
    assert_eq!(
        cell(&s, 3).cell.output.as_ref().unwrap().trust,
        Trust::Approximate
    );
    assert_eq!(
        cell(&s, 4).cell.calculated_revision,
        cell(&before, 4).cell.calculated_revision
    );
    assert_eq!(cell(&s, 2).cell.calculated_revision, Some(s.revision));
    assert_eq!(cell(&s, 3).dependencies, vec![1, 2]);
    assert_eq!(s.calculation_order, vec![1, 2, 3, 4]);
    assert_eq!(s.dependencies.len(), 3);
}
#[test]
fn assumptions_recalculate_transitive_users_and_preserve_unrelated_results() {
    let mut c = core();
    define(&mut c, None, "a", "x/x", false);
    define(&mut c, None, "b", "@a+1", false);
    let before = define(&mut c, None, "constant", "5", false);
    let s = edit(
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
    assert_eq!(cell(&s, 1).result_text.as_deref(), Some("1"));
    assert_eq!(cell(&s, 2).result_text.as_deref(), Some("2"));
    assert_eq!(
        cell(&s, 3).cell.calculated_revision,
        cell(&before, 3).cell.calculated_revision
    );
    assert!(cell(&s, 2).symbols.contains(&"x".into()));
}
#[test]
fn references_expand_definitions_before_calculus_and_preserve_bound_scope() {
    let mut c = core();
    define(&mut c, None, "f", "x^3", false);
    let s = define(&mut c, None, "derivative", r"\frac{d}{dx}(\cell{f})", true);
    assert_eq!(cell(&s, 2).result_text.as_deref(), Some("(3 * (x ^ 2))"));
    let s = define(&mut c, None, "area", r"\int_0^1 \cell{f} dx", true);
    assert_eq!(cell(&s, 3).result_text.as_deref(), Some("1/4"));
    assert!(cell(&s, 3).symbols.is_empty());
    let s = define(&mut c, Some(1), "f", "x^2", false);
    assert_eq!(cell(&s, 3).result_text.as_deref(), Some("1/3"));
    // Referencing a definition does not copy its separately selected transform output.
    transform(&mut c, 1, "differentiate", BTreeMap::new());
    let s = edit(&mut c, Command::Recalculate { id: Some(3) });
    assert_eq!(cell(&s, 3).result_text.as_deref(), Some("1/3"));
    define(&mut c, None, "ratio", "x/x", false);
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
    let s = define(
        &mut c,
        None,
        "singular",
        r"\int_{-1}^1 \cell{ratio} dx",
        true,
    );
    assert!(cell(&s, 5).cell.output.is_none());
    assert!(cell(&s, 5).cell.calculation_error.is_some());
}
#[test]
fn matrices_recover_after_upstream_shape_failure_and_old_results_disappear() {
    let mut c = core();
    define(
        &mut c,
        None,
        "A",
        r"\begin{bmatrix}1&2\\3&4\end{bmatrix}",
        true,
    );
    define(&mut c, None, "B", "@A*@A", false);
    let good = transform(&mut c, 2, "determinant", BTreeMap::new());
    assert_eq!(cell(&good, 2).result_text.as_deref(), Some("4"));
    let bad = define(
        &mut c,
        Some(1),
        "A",
        r"\begin{bmatrix}1&2\end{bmatrix}",
        true,
    );
    assert!(cell(&bad, 2).cell.output.is_none());
    assert_eq!(
        cell(&bad, 2).cell.calculation_error.as_deref(),
        Some("domain_error")
    );
    let restored = define(
        &mut c,
        Some(1),
        "A",
        r"\begin{bmatrix}1&0\\0&2\end{bmatrix}",
        true,
    );
    assert_eq!(cell(&restored, 2).result_text.as_deref(), Some("4"));
    assert!(cell(&restored, 2).cell.calculation_error.is_none());
}
#[test]
fn cycles_missing_references_and_deleting_providers_roll_back_atomically() {
    let mut c = core();
    define(&mut c, None, "a", "2", false);
    define(&mut c, None, "b", "@a+1", false);
    let before = serde_json::to_string(c.project()).unwrap();
    for command in [
        Command::Define {
            id: Some(1),
            name: "a".into(),
            source: "@b".into(),
        },
        Command::Define {
            id: None,
            name: "self_ref".into(),
            source: "@self_ref".into(),
        },
        Command::DefineLatex {
            id: None,
            name: "missing".into(),
            source: r"\cell{unknown}".into(),
        },
        Command::Remove { id: 1 },
    ] {
        assert!(
            c.apply(Request {
                expected_revision: c.project().revision,
                actor: Actor::Human,
                command
            })
            .is_err()
        );
        assert_eq!(serde_json::to_string(c.project()).unwrap(), before);
    }
    let mut corrupt = c.project().clone();
    corrupt.current_mut().cells[0].root = usize::MAX;
    assert!(corrupt.validate().is_err());
}
#[test]
fn rename_preserves_cell_ids_updates_only_reference_tokens_and_survives_reload() {
    let mut c = core();
    define(&mut c, None, "a", "2", false);
    define(&mut c, None, "ab", "3", false);
    define(&mut c, None, "b", "@a+@ab+a", false);
    define(&mut c, None, "tex", r"\cell{a}+\cell{ab}+a", true);
    let s = define(&mut c, Some(1), "renamed", "4", false);
    assert_eq!(cell(&s, 3).cell.source, "@renamed+@ab+a");
    assert_eq!(cell(&s, 4).cell.source, r"\cell{renamed}+\cell{ab}+a");
    let mut loaded =
        AppCore::new(serde_json::from_str(&serde_json::to_string(c.project()).unwrap()).unwrap())
            .unwrap();
    assert!(!loaded.dirty());
    let s = define(&mut loaded, Some(1), "renamed", "5", false);
    assert!(cell(&s, 3).result_text.as_ref().unwrap().contains('8'));
}
#[test]
fn recipes_bindings_branches_errors_and_undo_survive_document_roundtrip() {
    let mut c = core();
    define(&mut c, None, "a", "2", false);
    define(&mut c, None, "b", "@a/(x-1)", false);
    transform(&mut c, 2, "evaluate", BTreeMap::from([("x".into(), 2.0)]));
    edit(
        &mut c,
        Command::Branch {
            name: "experiment".into(),
        },
    );
    define(&mut c, Some(1), "a", "4", false);
    let json = serde_json::to_string(c.project()).unwrap();
    let mut c = AppCore::new(serde_json::from_str(&json).unwrap()).unwrap();
    let s = define(&mut c, Some(1), "a", "6", false);
    assert_eq!(cell(&s, 2).cell.output.as_ref().unwrap().value, Some(6.0));
    let s = edit(&mut c, Command::Undo);
    assert_eq!(cell(&s, 2).cell.output.as_ref().unwrap().value, Some(4.0));
    let s = edit(
        &mut c,
        Command::Checkout {
            name: "main".into(),
        },
    );
    assert_eq!(cell(&s, 1).result_text.as_deref(), Some("2"));
    let s = define(&mut c, Some(2), "b", "@a/(x-2)", false);
    assert!(cell(&s, 2).cell.output.is_none());
    assert_eq!(
        cell(&s, 2).cell.calculation_error.as_deref(),
        Some("domain_error")
    );
    let mut loaded =
        AppCore::new(serde_json::from_str(&serde_json::to_string(c.project()).unwrap()).unwrap())
            .unwrap();
    let s = define(&mut loaded, Some(2), "b", "@a/(x-1)", false);
    assert_eq!(cell(&s, 2).cell.output.as_ref().unwrap().value, Some(2.0));
}
#[test]
fn contextual_preview_is_pure_and_exchange_flattens_definition_references() {
    let mut c = core();
    define(
        &mut c,
        None,
        "A",
        r"\begin{bmatrix}1&2\\3&4\end{bmatrix}",
        true,
    );
    define(&mut c, None, "B", r"\cell{A}^{T}", true);
    let snapshot = c.snapshot();
    assert_eq!(
        cell(&snapshot, 2).result_text.as_deref(),
        Some("matrix(2x2; 1, 3, 2, 4)")
    );
    assert!(cell(&snapshot, 2).symbols.is_empty());
    let before = serde_json::to_string(c.project()).unwrap();
    assert_eq!(c.preview_latex(r"\cell{A}", None).unwrap().kind, "matrix");
    assert!(c.preview_latex(r"\cell{B}", Some(2)).is_err());
    assert_eq!(serde_json::to_string(c.project()).unwrap(), before);
    let imported = exchange::import(&before).unwrap();
    assert!(!imported.source.contains("\\cell"));
    let p = latex::preview(&imported.source).unwrap();
    assert_eq!(p.kind, "matrix");
    let src = latex::convert("@A+1", SourceFormat::Expression, SourceFormat::Latex).unwrap();
    let back = latex::convert(&src, SourceFormat::Latex, SourceFormat::Expression).unwrap();
    assert!(back.contains("@A"));
    let mut bad = c.project().clone();
    bad.current_mut().cells[1].resolved_root = Some(0);
    assert!(bad.validate().is_err());
    assert!(
        c.project()
            .graph
            .nodes
            .iter()
            .any(|node| matches!(node, Node::Reference(_)))
    );
}
#[test]
fn legacy_recipes_are_recovered_from_audited_parameters() {
    let mut c = core();
    define(&mut c, None, "a", "x^2", false);
    transform(&mut c, 1, "evaluate", BTreeMap::from([("x".into(), 3.0)]));
    let mut project = c.project().clone();
    project.current_mut().cells[0].calculation = None;
    let mut loaded = AppCore::new(project).unwrap();
    let s = define(&mut loaded, Some(1), "a", "x^3", false);
    assert_eq!(cell(&s, 1).cell.output.as_ref().unwrap().value, Some(27.0));
}

#[test]
fn legacy_recipe_recovery_does_not_cross_undo_history() {
    let mut c = core();
    define(&mut c, None, "a", "x^2", false);
    transform(&mut c, 1, "evaluate", BTreeMap::from([("x".into(), 3.0)]));
    transform(&mut c, 1, "evaluate", BTreeMap::from([("x".into(), 4.0)]));
    edit(&mut c, Command::Undo);
    let mut project = c.project().clone();
    project.current_mut().cells[0].calculation = None;
    let loaded = AppCore::new(project).unwrap();
    assert!(loaded.project().current().cells[0].calculation.is_none());
}
