use symbio_handwriting::{CancellationToken, Cancelled, Ink, Recognizer};
fn main() -> symbio_handwriting::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if !(4..=5).contains(&args.len())
        || args
            .get(4)
            .is_some_and(|s| s != "--beam" && s != "--cancel")
    {
        return Err("usage: recognize MODELS RUNTIME INK_JSON [--beam|--cancel]".into());
    }
    let ink: Ink = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let mut recognizer = Recognizer::load(
        std::path::Path::new(&args[1]),
        std::path::Path::new(&args[2]),
    )?;
    if args.get(4).is_some_and(|s| s == "--cancel") {
        let cancel = CancellationToken::default();
        cancel.cancel();
        let err = recognizer
            .recognize_candidates_with_cancel(&ink, &cancel)
            .unwrap_err();
        if !err.is::<Cancelled>() {
            return Err(err);
        }
        let retry = recognizer.recognize_candidates(&ink)?;
        println!("{}", serde_json::json!({"cancelled": true, "retry": retry}));
        return Ok(());
    }
    let result = if args.len() == 5 {
        recognizer.recognize_candidates(&ink)?
    } else {
        recognizer.recognize(&ink)?
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
