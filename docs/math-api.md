# Math operation API v1

GUIとCLIは同じ `symbio_core::Request` を実行します。CLIは標準入出力の改行区切りJSONです。MCP/JSON-RPC/HTTPサーバーではありません。リモート接続や認証を提供しません。

```sh
cargo run --locked -p symbio-cli -- init /tmp/study.symbio
cargo run --locked -p symbio-cli -- serve /tmp/study.symbio < examples/session.jsonl
cargo run --locked -p symbio-cli -- inspect /tmp/study.symbio
```

`init` は既存ファイルを上書きしません。同時にデスクトップで同じ文書を開かないでください。`serve` 内の変更は `math.save` が来るまでメモリ内のみです。

## Mutation envelope

```json
{
  "expected_revision": 0,
  "actor": "human",
  "command": {
    "method": "math.transform",
    "id": 1,
    "operation": "differentiate",
    "variable": "x",
    "bindings": {}
  }
}
```

成功時は `{"ok":true,"result":SNAPSHOT}`、失敗時は `{"ok":false,"error":"..."}` を1行返します。成功した変更のrevisionを次の要求に使用してください。失敗した操作はrevisionや文書を変更しません。`actor` は `human` または `ai` で、呼出元の自己申告です。

| Command | Fields | Meaning |
|---|---|---|
| `math.define` | `id: null` or cell ID, `name`, `source` | Add or replace a cell; clear its result |
| `math.define_latex` | `id: null` or cell ID, `name`, `source` | Parse bounded LaTeX into SEG, add or replace a cell; clear its result |
| `math.assume` | `symbol`, `assumption` | Set domain/nonzero/dimension; clear current branch results |
| `math.transform` | `id`, `operation`, `variable`, optional `bindings` | Explicit computation |
| `math.select` | `id` | Set selected cell |
| `math.remove` | `id` | Remove cell from current branch |
| `math.note` | `id`, `text` | Set research note |
| `math.branch` | `name` | Clone current branch and switch to it |
| `math.checkout` | `name` | Switch branch |
| `math.rename` | `name` | Rename project |
| `math.undo` / `math.redo` | none | Session-local history navigation; audit is retained |

Transform operations: `simplify`, `differentiate`, `integrate`, `expand`, `evaluate`. The last operation requires numeric bindings for every symbol. Unsupported transforms return an error; they do not substitute guessed output.

```json
{"domain":"real","nonzero":true,"dimension":[1,2,-2]}
```

Domains: `unspecified`, `real`, `nonnegative`, `positive`, `complex`. `dimension` is optional/null, otherwise integer powers `[mass,length,time]`, each in `[-16,16]`. This is dimension metadata, not a unit conversion API. Numeric bindings are real float64 even if a symbol permits complex values.

## CLI read/save requests

These top-level requests do not use the mutation envelope:

```json
{"method":"math.inspect"}
{"method":"math.inspect","id":1}
{"method":"math.preview_latex","source":"\\frac{x^{2}}{2}"}
{"method":"math.export"}
{"method":"math.save"}
```

`math.inspect` returns a bounded view with source, MathML, output, assumptions, revisions, engine and recent history. With a cell ID it returns that cell and reachable input/output SEG nodes. `math.export` returns the complete v1 Project, including all branches and the shared arena. `math.save` atomically persists the current Project without incrementing the mathematical revision.

The [request schema](request.schema.json) documents the mutation envelope. CLI read/save requests are described above. Serde remains the runtime authority. File format and operation API are versioned separately; no backward compatibility beyond v1 is promised yet.

## LaTeX and source preservation

`math.preview_latex` returns `{expression, mathml, symbols}` from an isolated Rust graph; it does not alter revisions, history, assumptions or the document arena. It checks supported syntax, not mathematical definedness. For example, previewing `\frac{1}{0}` renders a fraction, but evaluating it fails.

`math.define_latex` is a revision-checked mutation using the same envelope as `math.define`. It stores `source_format: "latex"`, the original LaTeX source and the parsed root. The result is an ordinary SEG consumed by `math.transform` and `math.inspect`, not a separate text-based calculator. Parsing or dimension errors roll back the transaction. Loading a file re-parses the source according to its format and checks the stored root. Existing v1 cells without `source_format` default to `expression`; new files are not promised readable by older binaries.

See the README for the explicit grammar subset and limits. No TeX interpreter, network renderer or arbitrary macros run. Unsupported notation fails rather than being dropped. The desktop exposes the same pure operation as the `preview_latex` Tauri command, with debounce and outdated-response suppression in the WebView.

## Trust and reproducibility

Symbolic transforms return `symbolic`, precision `exact`, source conditions and engine identifier. `exact` means coefficients were not numerically rounded; it is not a formal proof. Float64 output returns `approximate` and explicitly no certified error bound. The current release never issues `verified` or `certified_numeric`. The `conjecture` enum is reserved; there is no conjecture-generation endpoint.

History stores the last 256 commands with actor, branch, revision and engine, while the file stores full current branch contents. It is a bounded audit, not a complete event-sourced replay log. Exporting arbitrary source text does not execute shell, Python, Julia or network calls.
