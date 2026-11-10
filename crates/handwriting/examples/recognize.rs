use symbio_handwriting::{Ink, Recognizer};
fn main() -> symbio_handwriting::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if !(4..=5).contains(&args.len()) || args.get(4).is_some_and(|s| s != "--beam") {
        return Err("usage: recognize MODELS RUNTIME INK_JSON [--beam]".into());
    }
    let ink: Ink = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let mut recognizer = Recognizer::load(
        std::path::Path::new(&args[1]),
        std::path::Path::new(&args[2]),
    )?;
    let result = if args.len() == 5 {
        recognizer.recognize_candidates(&ink)?
    } else {
        recognizer.recognize(&ink)?
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
