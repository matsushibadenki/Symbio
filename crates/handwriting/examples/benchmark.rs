use std::{
    io::{self, BufRead},
    path::Path,
};
use symbio_handwriting::{Ink, Recognizer, Result};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: benchmark MODELS RUNTIME < ink.jsonl".into());
    }
    let mut recognizer = Recognizer::load(Path::new(&args[1]), Path::new(&args[2]))?;
    for line in io::stdin().lock().lines() {
        let ink: Ink = serde_json::from_str(&line?)?;
        let result = match recognizer.recognize_candidates(&ink) {
            Ok(result) => serde_json::to_value(result)?,
            Err(error) => serde_json::json!({"error": error.to_string()}),
        };
        println!("{}", result);
    }
    Ok(())
}
