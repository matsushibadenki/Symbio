//! Stateless, local stdio MCP tools. No document handles, paths, network or external CAS.
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, Read, Write},
};
use symbio_core::{Actor, AppCore, Assumption, Command, ENGINE, Project, Request, SourceFormat};
const VERSION: &str = "2025-11-25";
const MAX_FRAME: usize = 65536;
#[derive(Default)]
struct Server {
    initialized: bool,
    ready: bool,
}
fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
fn result(id: Value, value: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":value})
}
fn tool_result(value: Value, failed: bool) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":failed})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preview {
    source: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Calculate {
    source: String,
    #[serde(default)]
    source_format: SourceFormat,
    #[serde(default = "compute")]
    operation: String,
    #[serde(default = "variable")]
    variable: String,
    #[serde(default)]
    assumptions: BTreeMap<String, Assumption>,
    #[serde(default)]
    bindings: BTreeMap<String, f64>,
}
fn compute() -> String {
    "compute".into()
}
fn variable() -> String {
    "x".into()
}
fn apply(core: &mut AppCore, command: Command) -> symbio_core::Result<()> {
    core.apply(Request {
        expected_revision: core.project().revision,
        actor: Actor::Ai,
        command,
    })?;
    Ok(())
}
fn calculate(args: Calculate) -> symbio_core::Result<Value> {
    let mut core = AppCore::new(Project::empty("MCP scratch")?)?;
    for (symbol, assumption) in args.assumptions {
        apply(&mut core, Command::Assume { symbol, assumption })?;
    }
    let define = match args.source_format {
        SourceFormat::Expression => Command::Define {
            id: None,
            name: "f".into(),
            source: args.source,
        },
        SourceFormat::Latex => Command::DefineLatex {
            id: None,
            name: "f".into(),
            source: args.source,
        },
    };
    apply(&mut core, define)?;
    apply(
        &mut core,
        Command::Transform {
            id: 1,
            operation: args.operation,
            variable: args.variable,
            bindings: args.bindings,
        },
    )?;
    let cell = core.snapshot().cells.remove(0);
    Ok(json!({"ok":true,"engine":ENGINE,"cell":cell,"seg":core.inspect(1)?}))
}
fn call(name: &str, arguments: Value) -> Result<Value, ()> {
    let computed = match name {
        "math_preview_latex" => {
            let args: Preview = serde_json::from_value(arguments).map_err(|_| ())?;
            if args.source.is_empty() || args.source.len() > 2048 {
                return Err(());
            }
            symbio_core::latex::preview(&args.source).and_then(|p| {
                Ok(json!({"ok":true,"engine":ENGINE,"preview":serde_json::to_value(p)?}))
            })
        }
        "math_calculate" => {
            let args: Calculate = serde_json::from_value(arguments).map_err(|_| ())?;
            if args.source.is_empty()
                || args.source.len() > 2048
                || args.assumptions.len() > 64
                || args.bindings.len() > 64
                || (!args.bindings.is_empty() && args.operation != "evaluate")
                || !symbio_core::graph::valid_symbol(&args.variable)
                || ![
                    "compute",
                    "simplify",
                    "differentiate",
                    "integrate",
                    "expand",
                    "solve",
                    "determinant",
                    "inverse",
                    "transpose",
                    "evaluate",
                ]
                .contains(&args.operation.as_str())
            {
                return Err(());
            }
            calculate(args)
        }
        _ => return Err(()),
    };
    Ok(match computed {
        Ok(value) => tool_result(value, false),
        Err(e) => tool_result(
            json!({"ok":false,"engine":ENGINE,"error":{"code":e.code(),"message":e.to_string()}}),
            true,
        ),
    })
}
fn tools() -> Value {
    serde_json::from_str(include_str!("../../../docs/mcp-tools.json")).expect("bundled MCP schemas")
}
impl Server {
    fn handle(&mut self, v: Value) -> Option<Value> {
        let id = v.get("id").cloned();
        if !v.is_object()
            || v.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || !v.get("method").is_some_and(Value::is_string)
            || id
                .as_ref()
                .is_some_and(|i| !(i.is_string() || i.is_i64() || i.is_u64()))
            || v.get("params").is_some_and(|p| !p.is_object())
        {
            return Some(error(Value::Null, -32600, "Invalid Request"));
        }
        let method = v["method"].as_str().unwrap();
        let params = v.get("params").cloned().unwrap_or(json!({}));
        let Some(id) = id else {
            if method == "notifications/initialized" && self.initialized {
                self.ready = true;
            }
            return None;
        };
        if method == "ping" {
            return Some(result(id, json!({})));
        }
        if method == "initialize" {
            if self.initialized {
                return Some(error(id, -32600, "Already initialized"));
            }
            let valid = params.get("protocolVersion").is_some_and(Value::is_string)
                && params.get("capabilities").is_some_and(Value::is_object)
                && params.get("clientInfo").is_some_and(|c| {
                    c.get("name").is_some_and(Value::is_string)
                        && c.get("version").is_some_and(Value::is_string)
                });
            if !valid {
                return Some(error(id, -32602, "Invalid initialization parameters"));
            }
            let requested = params["protocolVersion"].as_str().unwrap();
            let negotiated = if [VERSION, "2025-06-18"].contains(&requested) {
                requested
            } else {
                VERSION
            };
            self.initialized = true;
            return Some(result(
                id,
                json!({"protocolVersion":negotiated,"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"symbio-math","version":env!("CARGO_PKG_VERSION")},"instructions":"Stateless local Rust calculations. No open-document access, filesystem writes, network or external CAS. Results are symbolic or approximate, never formal proof. Each call has a fresh scratch SEG."}),
            ));
        }
        if !self.ready {
            return Some(error(
                id,
                -32000,
                "Initialize and send notifications/initialized first",
            ));
        }
        Some(match method {
            "tools/list" if params.as_object().unwrap().keys().all(|k| k == "_meta") => {
                result(id, tools())
            }
            "tools/list" => error(id, -32602, "No pagination cursor is available"),
            "tools/call" => {
                if params
                    .as_object()
                    .unwrap()
                    .keys()
                    .any(|k| !["name", "arguments", "_meta"].contains(&k.as_str()))
                {
                    return Some(error(id, -32602, "Invalid tool parameters"));
                }
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(error(id, -32602, "Tool name required"));
                };
                match call(name, params.get("arguments").cloned().unwrap_or(json!({}))) {
                    Ok(value) => result(id, value),
                    Err(()) => error(id, -32602, "Unknown tool or invalid arguments"),
                }
            }
            _ => error(id, -32601, "Method not found"),
        })
    }
}
pub fn serve(
    input: impl BufRead,
    mut output: impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut input = input;
    let mut server = Server::default();
    loop {
        let mut frame = Vec::new();
        let n = (&mut input)
            .take((MAX_FRAME + 1) as u64)
            .read_until(b'\n', &mut frame)?;
        if n == 0 {
            return Ok(());
        }
        if n > MAX_FRAME {
            return Err("MCP frame exceeds 64 KiB".into());
        }
        let response = match serde_json::from_slice::<Value>(&frame) {
            Ok(v) => server.handle(v),
            Err(_) => Some(error(Value::Null, -32700, "Parse error")),
        };
        if let Some(mut response) = response {
            if serde_json::to_vec(&response)?.len() > 2 * 1024 * 1024 {
                response = error(response["id"].clone(), -32603, "Response exceeds 2 MiB");
            }
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(id: Value, method: &str, params: Value) -> Value {
        json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
    }
    fn ready() -> Server {
        let mut s = Server::default();
        let reply=s.handle(request(json!(1),"initialize",json!({"protocolVersion":VERSION,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}))).unwrap();
        assert_eq!(reply["result"]["protocolVersion"], VERSION);
        assert!(
            s.handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
                .is_none()
        );
        s
    }
    #[test]
    fn lifecycle_notifications_and_protocol_errors_are_distinct() {
        let mut s = Server::default();
        assert_eq!(
            s.handle(request(json!(1), "tools/list", json!({})))
                .unwrap()["error"]["code"],
            -32000
        );
        assert!(
            s.handle(json!({"jsonrpc":"2.0","method":"unknown"}))
                .is_none()
        );
        let mut s = ready();
        assert_eq!(
            s.handle(request(json!("id"), "unknown", json!({})))
                .unwrap()["error"]["code"],
            -32601
        );
        assert_eq!(
            s.handle(request(json!(2), "initialize", json!({})))
                .unwrap()["error"]["code"],
            -32600
        );
        assert_eq!(
            s.handle(request(Value::Null, "ping", json!({}))).unwrap()["error"]["code"],
            -32600
        );
        assert_eq!(
            s.handle(request(json!(3), "tools/list", json!({"cursor":"unknown"})))
                .unwrap()["error"]["code"],
            -32602
        );
    }
    #[test]
    fn scratch_tools_share_rust_calculus_assumptions_and_evidence() {
        let r = call(
            "math_calculate",
            json!({"source":"\\int_0^2 x^2\\,dx","source_format":"latex"}),
        )
        .unwrap();
        assert_eq!(r["isError"], false);
        assert_eq!(r["structuredContent"]["cell"]["result_text"], "8/3");
        assert_eq!(
            r["structuredContent"]["cell"]["output"]["trust"],
            "symbolic"
        );
        let r = call(
            "math_calculate",
            json!({"source":"sqrt(x^2)","assumptions":{"x":{"domain":"real"}}}),
        )
        .unwrap();
        assert_eq!(r["structuredContent"]["cell"]["result_text"], "abs(x)");
        let r = call("math_calculate", json!({"source":"sqrt(x^2)"})).unwrap();
        assert_ne!(r["structuredContent"]["cell"]["result_text"], "abs(x)");
        assert!(r["structuredContent"]["seg"].is_object());
        let r = call(
            "math_calculate",
            json!({"source":"x^2","operation":"evaluate","bindings":{"x":2.0}}),
        )
        .unwrap();
        assert_eq!(
            r["structuredContent"]["cell"]["output"]["trust"],
            "approximate"
        );
    }
    #[test]
    fn authority_is_bounded_and_tool_failures_are_structured() {
        for args in [
            json!({"source":"x","operation":"factor"}),
            json!({"source":"x","path":"/tmp/a"}),
            json!({"source":"x","source_format":"python"}),
            json!({"source":"x".repeat(2049)}),
        ] {
            assert!(call("math_calculate", args).is_err());
        }
        let r = call("math_calculate", json!({"source":"1/0"})).unwrap();
        assert_eq!(r["isError"], true);
        assert_eq!(r["structuredContent"]["error"]["code"], "domain_error");
        let r = call("math_calculate", json!({"source":"@f"})).unwrap();
        assert_eq!(r["isError"], true);
        let r = call("math_preview_latex", json!({"source":"\\frac{1}{2}"})).unwrap();
        assert_eq!(r["isError"], false);
        assert!(
            r["structuredContent"]["preview"]["mathml"]
                .as_str()
                .unwrap()
                .contains("mfrac")
        );
    }
    #[test]
    fn framing_is_jsonrpc_only_and_bounds_input() {
        let mut output = Vec::new();
        serve(
            &b"not json\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n"[..],
            &mut output,
        )
        .unwrap();
        let rows: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["error"]["code"], -32700);
        assert_eq!(rows[1]["id"], 7);
        assert!(serve(&vec![b'x'; MAX_FRAME + 1][..], Vec::new()).is_err());
    }
}
