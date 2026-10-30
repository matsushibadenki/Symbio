use std::io::{self, BufRead, Read, Write};
use symbio_core::{AppCore, Project, ProjectStore, Request};
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1)
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "export-mathjson") && args.len() == 3 {
        let mut store = ProjectStore::open(&args[1])?;
        let core = AppCore::new(store.load()?.ok_or("project not found")?)?;
        let id = core.project().current().selected.ok_or("select a cell")?;
        symbio_core::mathjson::write_new(
            &args[2],
            &symbio_core::mathjson::export(core.project(), id)?,
        )?;
        return Ok(());
    }
    if args.first().is_some_and(|s| s == "import-mathjson") && args.len() == 3 {
        let mut core = symbio_core::mathjson::import(&symbio_core::mathjson::read(&args[1])?)?;
        let mut store = ProjectStore::open(&args[2])?;
        if store.load()?.is_some() {
            return Err("destination exists".into());
        }
        store.save(core.project())?;
        core.mark_saved();
        println!("{}", serde_json::to_string_pretty(&core.snapshot())?);
        return Ok(());
    }
    if args.first().is_some_and(|s| s == "verify-reproduction") && args.len() == 2 {
        let report =
            symbio_core::reproduction::verify(&symbio_core::reproduction::read(&args[1])?)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        if !report.reproduced {
            return Err("reproduction incomplete or mismatched".into());
        }
        return Ok(());
    }
    if args.first().is_some_and(|s| s == "export-reproduction") && args.len() == 3 {
        let mut store = ProjectStore::open(&args[1])?;
        let core = AppCore::new(store.load()?.ok_or("project not found")?)?;
        let package = symbio_core::reproduction::capture(core.project())?;
        symbio_core::reproduction::write_new(&args[2], &package)?;
        println!("{}", package.sha256);
        return Ok(());
    }
    if args.len() != 2 || !["init", "inspect", "serve"].contains(&args[0].as_str()) {
        return Err("usage: symbio-cli import-mathjson JSON NEW_PROJECT | export-mathjson PROJECT JSON | init|inspect|serve PROJECT | export-reproduction PROJECT PACKAGE | verify-reproduction PACKAGE".into());
    }
    let mut store = ProjectStore::open(&args[1])?;
    let project = match store.load()? {
        Some(p) => {
            if args[0] == "init" {
                return Err("project already exists".into());
            }
            p
        }
        None if args[0] == "init" => {
            let p = Project::new("Polynomial study")?;
            store.save(&p)?;
            p
        }
        None => return Err("project not found".into()),
    };
    let mut core = AppCore::new(project)?;
    if args[0] != "serve" {
        println!("{}", serde_json::to_string_pretty(&core.snapshot())?);
        return Ok(());
    }
    // Newline-delimited JSON protocol, explicit caller-requested writes; no networking.
    let mut input = io::stdin().lock();
    loop {
        let mut line = Vec::new();
        let n = (&mut input).take(65537).read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        if n > 65536 {
            return Err("request exceeds 64 KiB".into());
        }
        let response = (|| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
            let v: serde_json::Value = serde_json::from_slice(&line)?;
            match v.get("method").and_then(|v| v.as_str()) {
                Some("math.compare_branches") => {
                    #[derive(serde::Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Compare {
                        method: String,
                        base: String,
                        target: String,
                    }
                    let request: Compare = serde_json::from_value(v)?;
                    debug_assert_eq!(request.method, "math.compare_branches");
                    Ok(serde_json::to_value(symbio_core::comparison::compare(
                        core.project(),
                        &request.base,
                        &request.target,
                    )?)?)
                }
                Some("math.inspect") => {
                    if let Some(id) = v.get("id").and_then(|v| v.as_u64()) {
                        Ok(core.inspect(id)?)
                    } else {
                        Ok(serde_json::to_value(core.snapshot())?)
                    }
                }
                Some("math.preview_latex") => {
                    let source = v
                        .get("source")
                        .and_then(|s| s.as_str())
                        .ok_or("source must be a string")?;
                    Ok(serde_json::to_value(core.preview_latex(
                        source,
                        v.get("id").and_then(|id| id.as_u64()),
                    )?)?)
                }
                Some("math.export") => Ok(serde_json::to_value(core.project())?),
                Some("math.save") => {
                    store.save(core.project())?;
                    core.mark_saved();
                    Ok(serde_json::to_value(core.snapshot())?)
                }
                _ => {
                    let request: Request = serde_json::from_value(v)?;
                    Ok(serde_json::to_value(core.apply(request)?)?)
                }
            }
        })();
        let out = match response {
            Ok(v) => serde_json::json!({"ok":true,"result":v}),
            Err(e) => serde_json::json!({"ok":false,"error":e.to_string()}),
        };
        println!("{out}");
        io::stdout().flush()?;
    }
    Ok(())
}
