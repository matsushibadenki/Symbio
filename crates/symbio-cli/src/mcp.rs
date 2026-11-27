//! Local stdio MCP tools with optional access to a startup-pinned project snapshot.
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
};
use symbio_core::{AppCore, ProjectStore, Snapshot};
use symbio_research::scratch;
const VERSION: &str = "2025-11-25";
const MAX_FRAME: usize = 65536;
#[derive(Default)]
struct Server {
    initialized: bool,
    ready: bool,
    notebook: Option<Notebook>,
}
fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
fn result(id: Value, value: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":value})
}
fn tool_result(value: Value) -> Value {
    let failed = value["ok"] == false;
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":failed})
}
fn call(name: &str, arguments: Value) -> Result<Value, ()> {
    scratch::execute(name, arguments)
        .map(tool_result)
        .map_err(|_| ())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NotebookRead {
    cell_id: Option<u64>,
    query: Option<String>,
    expected_revision: Option<u64>,
}

const NOTEBOOK_TOOL: &str = "math_read_notebook";

struct Notebook {
    snapshot: Snapshot,
    follow_path: Option<PathBuf>,
}

impl Notebook {
    fn refresh(&self) -> symbio_core::Result<Option<Snapshot>> {
        let Some(path) = &self.follow_path else {
            return Ok(None);
        };
        let mut project = ProjectStore::read_snapshot(path)?;
        if project.id != self.snapshot.project_id
            || !project.branches.contains_key(&self.snapshot.active)
        {
            return Err(symbio_core::Error::ExternalChange);
        }
        project.active.clone_from(&self.snapshot.active);
        Ok(Some(AppCore::new(project)?.snapshot()))
    }
}

fn read_notebook(access: &Notebook, args: Value) -> Result<Value, ()> {
    if args
        .get("cell_id")
        .is_some_and(|value| value.as_u64().is_none_or(|id| id == 0))
    {
        return Err(());
    }
    if args
        .get("expected_revision")
        .is_some_and(|value| !value.is_u64())
        || args.get("query").is_some_and(|value| {
            value.as_str().is_none_or(|value| {
                value.len() > 48 || !value.bytes().all(|byte| (b' '..=b'~').contains(&byte))
            })
        })
    {
        return Err(());
    }
    let args: NotebookRead = serde_json::from_value(args).map_err(|_| ())?;
    let query = args.query.as_deref().map(str::trim);
    if query.is_some_and(|query| query.is_empty() || query.len() > 48)
        || (query.is_some() && args.cell_id.is_some())
    {
        return Err(());
    }
    let fresh = match access.refresh() {
        Ok(fresh) => fresh,
        Err(_) => {
            return Ok(tool_result(
                json!({"ok":false,"error":{"code":"notebook_unavailable"}}),
            ));
        }
    };
    let notebook = fresh.as_ref().unwrap_or(&access.snapshot);
    if args
        .expected_revision
        .is_some_and(|revision| revision != notebook.revision)
    {
        return Ok(tool_result(
            json!({"ok":false,"error":{"code":"revision_conflict"}}),
        ));
    }
    let mut data = json!({
        "ok": true, "branch": notebook.active, "revision": notebook.revision,
    });
    if let Some(id) = args.cell_id {
        let Some(cell) = notebook.cells.iter().find(|view| view.cell.id == id) else {
            return Ok(tool_result(
                json!({"ok":false,"error":{"code":"cell_not_found"}}),
            ));
        };
        data["cell"] = json!({
            "id": id, "name": cell.cell.name, "source": cell.cell.source,
            "source_format": cell.cell.source_format, "mathml": cell.definition_mathml,
            "result_text": cell.result_text, "result_mathml": cell.result_mathml,
            "output": cell.cell.output.as_ref().map(|output| json!({
                "operation": output.operation, "trust": output.trust, "precision": output.precision,
                "conditions": output.conditions, "engine": output.engine,
            })),
            "calculation_error": cell.cell.calculation_error,
            "dependencies": cell.dependencies,
        });
    } else {
        if query.is_none() {
            data["assumptions"] = serde_json::to_value(&notebook.assumptions).map_err(|_| ())?;
        }
        let query = query.map(str::to_ascii_lowercase);
        data["cells"] = Value::Array(
            notebook
                .cells
                .iter()
                .filter(|view| {
                    query
                        .as_ref()
                        .is_none_or(|query| view.cell.name.to_ascii_lowercase().contains(query))
                })
                .map(|view| json!({"id":view.cell.id,"name":view.cell.name}))
                .collect(),
        );
    }
    Ok(tool_result(data))
}

impl Server {
    fn tool_schema(&self) -> Result<Value, ()> {
        let mut schema = scratch::schema().map_err(|_| ())?;
        if self.notebook.is_some() {
            let tool = serde_json::from_str(include_str!("../../../docs/mcp-notebook-tool.json"))
                .map_err(|_| ())?;
            schema["tools"].as_array_mut().ok_or(())?.push(tool);
        }
        Ok(schema)
    }

    fn handle(&mut self, v: Value) -> Option<Value> {
        let id = v.get("id").cloned();
        let Some(method) = v.get("method").and_then(Value::as_str) else {
            return Some(error(Value::Null, -32600, "Invalid Request"));
        };
        if !v.is_object()
            || v.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || id
                .as_ref()
                .is_some_and(|i| !(i.is_string() || i.is_i64() || i.is_u64()))
            || v.get("params").is_some_and(|p| !p.is_object())
        {
            return Some(error(Value::Null, -32600, "Invalid Request"));
        }
        let empty_params = serde_json::Map::new();
        let params = v
            .get("params")
            .and_then(Value::as_object)
            .unwrap_or(&empty_params);
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
            let Some(requested) = params.get("protocolVersion").and_then(Value::as_str) else {
                return Some(error(id, -32602, "Invalid initialization parameters"));
            };
            let valid = params.get("capabilities").is_some_and(Value::is_object)
                && params.get("clientInfo").is_some_and(|c| {
                    c.get("name").is_some_and(Value::is_string)
                        && c.get("version").is_some_and(Value::is_string)
                });
            if !valid {
                return Some(error(id, -32602, "Invalid initialization parameters"));
            }
            let negotiated = if [VERSION, "2025-06-18"].contains(&requested) {
                requested
            } else {
                VERSION
            };
            self.initialized = true;
            let instructions = if self.notebook.is_some() {
                "Read the explicitly authorized saved branch with math_read_notebook. Math tools calculate in separate scratch state. No notebook writes."
            } else {
                "Stateless local Rust calculations. No document access. Each call has a fresh scratch SEG."
            };
            return Some(result(
                id,
                json!({"protocolVersion":negotiated,"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"symbio-math","version":env!("CARGO_PKG_VERSION")},"instructions":instructions}),
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
            "tools/list" if params.keys().all(|k| k == "_meta") => match self.tool_schema() {
                Ok(schema) => result(id, schema),
                Err(_) => error(id, -32603, "Tool schema unavailable"),
            },
            "tools/list" => error(id, -32602, "No pagination cursor is available"),
            "tools/call" => {
                if params
                    .keys()
                    .any(|k| !["name", "arguments", "_meta"].contains(&k.as_str()))
                {
                    return Some(error(id, -32602, "Invalid tool parameters"));
                }
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(error(id, -32602, "Tool name required"));
                };
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                let response = if name == NOTEBOOK_TOOL {
                    self.notebook
                        .as_ref()
                        .ok_or(())
                        .and_then(|snapshot| read_notebook(snapshot, arguments))
                } else {
                    call(name, arguments)
                };
                match response {
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
    serve_snapshot(input, &mut output, None)
}

pub fn serve_project(
    input: impl BufRead,
    output: impl Write,
    path: impl AsRef<Path>,
    branch: Option<&str>,
    follow: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let path = parent
        .canonicalize()?
        .join(path.file_name().ok_or("invalid project path")?);
    let mut project = ProjectStore::read_snapshot(&path)?;
    if let Some(branch) = branch {
        if !project.branches.contains_key(branch) {
            return Err("the requested saved branch does not exist".into());
        }
        project.active = branch.into();
    }
    let notebook = Notebook {
        snapshot: AppCore::new(project)?.snapshot(),
        follow_path: follow.then_some(path),
    };
    serve_snapshot(input, output, Some(notebook))
}

fn serve_snapshot(
    input: impl BufRead,
    mut output: impl Write,
    notebook: Option<Notebook>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut input = input;
    let mut server = Server {
        notebook,
        ..Server::default()
    };
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
