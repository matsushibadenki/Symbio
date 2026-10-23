use symbio_core::{
    graph::{MAX_NODES, Node},
    *,
};
fn apply(c: &mut AppCore, command: Command) -> Snapshot {
    c.apply(Request {
        expected_revision: c.project().revision,
        actor: Actor::Human,
        command,
    })
    .unwrap()
}
fn junk(p: &mut Project, count: usize) {
    for i in 0..count {
        p.graph
            .intern(Node::Number((100000 + i).to_string()))
            .unwrap();
    }
}
fn meaning(p: &Project) -> serde_json::Value {
    serde_json::to_value(p.branches.iter().map(|(name,b)|(name,b.cells.iter().map(|c|serde_json::json!({"id":c.id,"source":c.source,"format":c.source_format,"expression":p.graph.text(c.root),"resolved":c.resolved_root.map(|r|p.graph.text(r)),"output":c.output.as_ref().map(|o|(o.root.map(|r|p.graph.mathml(r)),o.value,&o.operation,&o.trust,&o.precision,&o.conditions)),"recipe":c.calculation,"error":c.calculation_error,"revision":c.calculated_revision})).collect::<Vec<_>>())).collect::<Vec<_>>()).unwrap()
}
#[test]
fn collection_preserves_all_branch_inputs_resolved_definitions_and_output_roots() {
    let mut p = Project::new("Collect").unwrap();
    junk(&mut p, 500);
    let mut c = AppCore::new(p).unwrap();
    for (name, source) in [
        ("integral", r"\int_{0}^{1} x^2 \,dx"),
        ("matrix", r"\begin{bmatrix}1&2\\3&4\end{bmatrix}"),
        ("sum", r"\sum_{k=1}^{4} k^2"),
        ("equation", r"x^2=4"),
        ("reference", r"\cell{sum}+1"),
    ] {
        apply(
            &mut c,
            Command::DefineLatex {
                id: None,
                name: name.into(),
                source: source.into(),
            },
        );
    }
    apply(
        &mut c,
        Command::Branch {
            name: "alternative".into(),
        },
    );
    apply(
        &mut c,
        Command::Define {
            id: Some(1),
            name: "f".into(),
            source: "x^4+77".into(),
        },
    );
    let before = meaning(c.project());
    let size = serde_json::to_vec(c.project()).unwrap().len();
    let count = c.snapshot().node_count;
    let s = apply(&mut c, Command::Compact {});
    assert!(s.node_count < count - 400);
    assert_eq!(s.graph_stats.unused, 0);
    assert_eq!(meaning(c.project()), before);
    assert!(serde_json::to_vec(c.project()).unwrap().len() < size);
    c.project().validate().unwrap();
    apply(&mut c, Command::Undo);
    assert_eq!(meaning(c.project()), before);
    assert_eq!(c.snapshot().node_count, count);
    apply(&mut c, Command::Redo);
    assert_eq!(meaning(c.project()), before);
    assert_eq!(c.snapshot().graph_stats.unused, 0);
    let dir = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(dir.path().join("collected.symbio")).unwrap();
    store.save(c.project()).unwrap();
    let mut loaded = AppCore::new(store.load().unwrap().unwrap()).unwrap();
    assert_eq!(meaning(loaded.project()), before);
    let s = apply(
        &mut loaded,
        Command::DefineLatex {
            id: Some(5),
            name: "sum".into(),
            source: r"\sum_{k=1}^{5} k^2".into(),
        },
    );
    assert_eq!(
        s.cells
            .iter()
            .find(|c| c.cell.name == "reference")
            .unwrap()
            .result_text
            .as_deref(),
        Some("56")
    );
}
#[test]
fn automatic_collection_bounds_dead_nodes_and_limit_retry_can_still_edit() {
    let mut p = Project::new("Automatic").unwrap();
    junk(&mut p, 2200);
    let mut c = AppCore::new(p).unwrap();
    let before = meaning(c.project());
    let s = apply(&mut c, Command::Select { id: 1 });
    assert!(s.node_count < 100);
    assert_eq!(meaning(c.project()), before);
    let mut p = Project::new("Full").unwrap();
    let count = p.graph.nodes.len();
    junk(&mut p, MAX_NODES - count);
    let mut c = AppCore::new(p).unwrap();
    let before = serde_json::to_vec(c.project()).unwrap();
    assert!(c.preview_latex(r"\frac{x+12345}{2}", Some(1)).is_ok());
    assert_eq!(serde_json::to_vec(c.project()).unwrap(), before);
    let s = apply(
        &mut c,
        Command::Define {
            id: Some(1),
            name: "f".into(),
            source: "x^2+9999".into(),
        },
    );
    assert!(s.node_count < 100);
    assert_eq!(s.cells[0].result_text.as_deref(), Some("(2 * x)"));
}
#[test]
fn failed_retry_and_invalid_collection_never_publish_partial_graphs() {
    let mut p = Project::new("Rollback").unwrap();
    let count = p.graph.nodes.len();
    junk(&mut p, MAX_NODES - count);
    let mut c = AppCore::new(p).unwrap();
    let before = serde_json::to_vec(c.project()).unwrap();
    let revision = c.project().revision;
    let result = c.apply(Request {
        expected_revision: revision,
        actor: Actor::Human,
        command: Command::Define {
            id: Some(1),
            name: "f".into(),
            source: "@absent+1".into(),
        },
    });
    assert!(matches!(result, Err(Error::MissingReference(_))));
    assert_eq!(serde_json::to_vec(c.project()).unwrap(), before);
    let mut bad = c.project().clone();
    bad.current_mut().cells[0].root = MAX_NODES + 1;
    let before = serde_json::to_vec(&bad).unwrap();
    assert!(collection::compact(&mut bad).is_err());
    assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
}

#[test]
fn collection_request_matches_the_closed_schema() {
    let request = serde_json::json!({"expected_revision":0,"actor":"human","command":{"method":"math.compact"}});
    assert!(serde_json::from_value::<Request>(request.clone()).is_ok());
    let mut invalid = request;
    invalid["command"]["unexpected"] = true.into();
    assert!(serde_json::from_value::<Request>(invalid).is_err());
}
