use std::collections::BTreeMap;
use symbio_core::{graph::Graph, *};

fn apply(core: &mut AppCore, command: Command) -> Result<Snapshot> {
    core.apply(Request {
        expected_revision: core.project().revision,
        actor: Actor::Human,
        command,
    })
}
fn define(core: &mut AppCore, source: &str) -> Snapshot {
    apply(
        core,
        Command::DefineLatex {
            id: Some(1),
            name: "f".into(),
            source: source.into(),
        },
    )
    .unwrap()
}
fn transform(core: &mut AppCore, operation: &str) -> Snapshot {
    apply(
        core,
        Command::Transform {
            id: 1,
            operation: operation.into(),
            variable: "x".into(),
            bindings: BTreeMap::from([("x".into(), 2.0)]),
        },
    )
    .unwrap()
}

#[test]
fn latex_and_expression_share_identical_seg() {
    for (latex, plain) in [
        (r"\frac{x+1}{2}", "(x+1)/2"),
        (r"\sqrt{x^{2}}", "sqrt(x^2)"),
        (r"x^3+2x^2-5x", "x^3+2*x^2-5*x"),
        (r"-x^{2}", "-x^2"),
        (r"x^{-2}", "x^(-2)"),
        (r"\sin^{2}(x)+\cos^{2}{x}", "sin(x)^2+cos(x)^2"),
        (r"\ln(\exp{x})", "log(exp(x))"),
        (r"2\left(x+1\right)", "2*(x+1)"),
        (r"\left|x\right|", "abs(x)"),
        (r"\lvert x\rvert", "abs(x)"),
        (r"\alpha \cdot x", "alpha*x"),
        (r"xy", "x*y"),
        (r"\[\frac{1}{3}+\frac{1}{6}\]", "1/3+1/6"),
        (r"$$\dfrac{x}{2}$$", "x/2"),
        (r"\(x^{2^{3}}\)", "x^(2^3)"),
    ] {
        let mut graph = Graph::default();
        let a = latex::parse(&mut graph, latex).unwrap();
        let b = parser::parse(&mut graph, plain).unwrap();
        assert_eq!(a, b, "{latex}");
        graph.rebuild().unwrap();
    }
}

#[test]
fn latex_preview_has_safe_mathml_and_no_project_effect() {
    let core = AppCore::new(Project::new("test").unwrap()).unwrap();
    let before = serde_json::to_value(core.project()).unwrap();
    let preview = latex::preview(r"\frac{x+1}{\sqrt{2}}").unwrap();
    assert!(preview.mathml.contains("<mfrac>"));
    assert!(preview.mathml.contains("<msqrt>"));
    assert_eq!(preview.symbols, ["x"]);
    assert_eq!(before, serde_json::to_value(core.project()).unwrap());
}

#[test]
fn latex_computes_exact_symbolic_and_approximate_results() {
    let mut core = AppCore::new(Project::new("test").unwrap()).unwrap();
    define(&mut core, r"\frac{1}{3}+\frac{1}{6}");
    assert_eq!(
        transform(&mut core, "simplify").cells[0]
            .result_text
            .as_deref(),
        Some("1/2")
    );
    define(&mut core, r"x^{3}+2x^{2}-5x");
    let derivative = transform(&mut core, "differentiate");
    let root = derivative.cells[0]
        .cell
        .output
        .as_ref()
        .unwrap()
        .root
        .unwrap();
    assert_eq!(
        core.project()
            .graph
            .numeric(root, &BTreeMap::from([("x".into(), 2.0)]))
            .unwrap(),
        15.0
    );
    define(&mut core, r"\frac{x^{2}}{2}");
    let integral = transform(&mut core, "integrate");
    let root = integral.cells[0]
        .cell
        .output
        .as_ref()
        .unwrap()
        .root
        .unwrap();
    assert!(
        (core
            .project()
            .graph
            .numeric(root, &BTreeMap::from([("x".into(), 3.0)]))
            .unwrap()
            - 4.5)
            .abs()
            < 1e-12
    );
    let evaluated = transform(&mut core, "evaluate");
    let out = evaluated.cells[0].cell.output.as_ref().unwrap();
    assert_eq!(out.value, Some(2.0));
    assert_eq!(out.trust, Trust::Approximate);
    define(&mut core, r"\sqrt{x^{2}}");
    assert_eq!(
        transform(&mut core, "simplify").cells[0]
            .result_text
            .as_deref(),
        Some("abs(x)")
    );
    define(&mut core, r"\frac{x}{x}");
    assert_ne!(
        transform(&mut core, "simplify").cells[0]
            .result_text
            .as_deref(),
        Some("1")
    );
}

#[test]
fn unsupported_and_malformed_latex_is_never_partially_accepted() {
    for input in [
        "",
        r"\frac{x}",
        r"\frac12",
        r"\sqrt[3]{x}",
        r"\sin x",
        r"\sin(x",
        r"\left(x]",
        r"\left(x)",
        r"x^23",
        r"x^2^3",
        r"\int_0^1 x dx",
        r"x=1",
        r"x_1",
        r"\pi",
        r"\input{file}",
        r"\href{url}{x}",
        r"\htmlClass{a}{x}",
        r"\def\x{2}",
        "<script>",
        r"x%comment",
        r"$x",
        r"\right)x",
        r"\",
        r".",
        r"1..2",
    ] {
        assert!(latex::preview(input).is_err(), "accepted {input}");
    }
    assert!(latex::preview(&"x".repeat(2049)).is_err());
    assert!(latex::preview(&format!("{}x{}", "{".repeat(70), "}".repeat(70))).is_err());
    let mut core = AppCore::new(Project::new("test").unwrap()).unwrap();
    let before = serde_json::to_value(core.project()).unwrap();
    assert!(
        apply(
            &mut core,
            Command::DefineLatex {
                id: Some(1),
                name: "f".into(),
                source: r"\frac{x}{".into()
            }
        )
        .is_err()
    );
    assert_eq!(before, serde_json::to_value(core.project()).unwrap());
}

#[test]
fn latex_format_roundtrips_and_legacy_documents_load() {
    let p = Project::new("legacy").unwrap();
    let mut legacy = serde_json::to_value(&p).unwrap();
    for cell in legacy["branches"]["main"]["cells"].as_array_mut().unwrap() {
        cell.as_object_mut().unwrap().remove("source_format");
    }
    let mut core = AppCore::new(serde_json::from_value(legacy).unwrap()).unwrap();
    define(&mut core, r"\frac{x+1}{2}");
    let dir = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(dir.path().join("latex.symbio")).unwrap();
    store.save(core.project()).unwrap();
    let loaded = AppCore::new(store.load().unwrap().unwrap()).unwrap();
    assert_eq!(
        loaded.snapshot().cells[0].cell.source_format,
        SourceFormat::Latex
    );
    assert_eq!(loaded.snapshot().cells[0].cell.source, r"\frac{x+1}{2}");
    apply(&mut core, Command::Undo).unwrap();
    assert_eq!(
        core.snapshot().cells[0].cell.source_format,
        SourceFormat::Expression
    );
    apply(&mut core, Command::Redo).unwrap();
    assert_eq!(
        core.snapshot().cells[0].cell.source_format,
        SourceFormat::Latex
    );
    let mut tampered = serde_json::to_value(core.project()).unwrap();
    tampered["branches"]["main"]["cells"][0]["source_format"] = "expression".into();
    assert!(AppCore::new(serde_json::from_value(tampered).unwrap()).is_err());
}
