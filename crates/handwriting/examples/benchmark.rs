use std::{
    io::{self, BufRead},
    path::Path,
};
use symbio_handwriting::{Ink, Recognizer, Result};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let conditional_paired = args.get(3).is_some_and(|arg| arg == "--conditional-paired");
    let paired = conditional_paired || args.get(3).is_some_and(|arg| arg == "--paired");
    if args.len() != 3 && !(args.len() == 4 && paired) {
        return Err(
            "usage: benchmark MODELS RUNTIME [--paired|--conditional-paired] < ink.jsonl".into(),
        );
    }
    let mut recognizer = Recognizer::load(Path::new(&args[1]), Path::new(&args[2]))?;
    for line in io::stdin().lock().lines() {
        let ink: Ink = serde_json::from_str(&line?)?;
        let result = match recognizer.recognize_candidates_raw(&ink) {
            Ok(mut result) if paired => {
                if conditional_paired {
                    result.rank_for_notebook_with_ink(Some(&ink), false);
                }
                let before = serde_json::to_value(&result)?;
                result.rank_for_notebook_with_ink(Some(&ink), true);
                serde_json::json!({"before": before, "after": result})
            }
            Ok(mut result) => {
                result.rank_for_notebook_with_ink(Some(&ink), true);
                serde_json::to_value(result)?
            }
            Err(error) => serde_json::json!({"error": error.to_string()}),
        };
        println!("{}", result);
    }
    Ok(())
}
