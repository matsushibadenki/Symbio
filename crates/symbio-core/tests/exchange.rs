use std::collections::BTreeMap;
use symbio_core::{exchange::*, *};
fn input(source: &str, operation: &str) -> MathInput {
    MathInput {
        source: source.into(),
        operation: operation.into(),
        variable: "x".into(),
        bindings: BTreeMap::new(),
        assumptions: BTreeMap::new(),
    }
}
#[test]
fn shared_kernel_computes_without_replacing_source() {
    for (source, operation, expected) in [
        (r"\int_0^1 x^2 dx", "compute", "1/3"),
        (r"\begin{bmatrix}1&2\\3&4\end{bmatrix}", "determinant", "-2"),
        ("x^2-5x+6=0", "solve", "x in {2, 3}"),
    ] {
        let result = compute(&input(source, operation)).unwrap();
        assert_eq!(result.cell.source, source);
        assert_eq!(result.result_text.as_deref(), Some(expected));
        assert_eq!(result.cell.output.unwrap().trust, Trust::Symbolic);
    }
    let result = compute(&input("2/3", "evaluate")).unwrap();
    assert_eq!(result.cell.output.unwrap().trust, Trust::Approximate);
}
#[test]
fn single_equation_exchange_is_valid_and_does_not_copy_other_state() {
    let original = input(r"\int_0^1 x^2 dx", "compute");
    let json = export(
        &original,
        Some(MathOrigin {
            document_id: uuid::Uuid::new_v4(),
            node_id: uuid::Uuid::new_v4(),
        }),
    )
    .unwrap();
    let p: Project = serde_json::from_str(&json).unwrap();
    p.validate().unwrap();
    assert_eq!(p.branches.len(), 1);
    assert_eq!(p.current().cells.len(), 1);
    assert!(p.current().cells[0].output.is_none());
    assert!(p.current().assumptions.is_empty());
    assert_eq!(import(&json).unwrap().source, original.source);
    let legacy = serde_json::to_string(&Project::new("legacy").unwrap()).unwrap();
    assert!(import(&legacy).unwrap().source.contains("^{"));
}
#[test]
fn corrupt_files_and_oversized_or_unsupported_math_fail() {
    assert!(import("{}").is_err());
    assert!(compute(&input(r"\int 1/x dx", "compute")).is_err());
    assert!(compute(&input("x^2", "execute_shell")).is_err());
    assert!(export(&input(&"x".repeat(2049), "compute"), None).is_err());
    let mut p = Project::new("bad").unwrap();
    p.current_mut().cells[0].root = 999999;
    assert!(import(&serde_json::to_string(&p).unwrap()).is_err());
}
