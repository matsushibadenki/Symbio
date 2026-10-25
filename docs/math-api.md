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
| `math.assume` | `symbol`, `assumption` | Set domain/nonzero/dimension and recalculate affected cells |
| `math.recalculate` | optional `id` or null | Recalculate this cell and downstream consumers, or all cells |
| `math.transform` | `id`, `operation`, `variable`, optional `bindings` | Explicit computation |
| `math.select` | `id` | Set selected cell |
| `math.remove` | `id` | Remove cell from current branch |
| `math.note` | `id`, `text` | Set research note |
| `math.branch` | `name` | Clone current branch and switch to it |
| `math.checkout` | `name` | Switch branch |
| `math.rename` | `name` | Rename project |
| `math.undo` / `math.redo` | none | Session-local history navigation; audit is retained |

Transform operations: `compute` (alias of `simplify`), `simplify`, `differentiate`, `integrate`, `expand`, `evaluate`, `solve`, `determinant`, `inverse`, `transpose`. `compute` evaluates integral/derivative/finite aggregate nodes and matrix arithmetic; equations remain relations until `solve`. `evaluate` requires bindings for free symbols and returns float64 approximations, including matrix entries. Bound integration/summation variables are not free symbols. Indefinite integrals require choosing an integration constant before numeric evaluation. Unsupported transforms return an error; they do not substitute guessed output.

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

`math.preview_latex` returns `{expression, mathml, symbols, kind}` from an isolated Rust graph; it does not alter revisions, history, assumptions or the document arena. It checks supported syntax, not mathematical definedness. For example, previewing `\frac{1}{0}` renders a fraction, but evaluating it fails.

`math.define_latex` is a revision-checked mutation using the same envelope as `math.define`. It stores `source_format: "latex"`, the original LaTeX source and the parsed root. The result is an ordinary SEG consumed by `math.transform` and `math.inspect`, not a separate text-based calculator. Parsing, shape or dimension errors roll back the transaction. Loading a file re-parses the source according to its format and checks the stored root. Existing v1 cells without `source_format` default to `expression`; new files are not promised readable by older binaries.

See the README for the explicit grammar subset and limits. No TeX interpreter, network renderer or arbitrary macros run. Unsupported notation fails rather than being dropped. The desktop exposes the same pure operation as the `preview_latex` Tauri command, with debounce and outdated-response suppression in the WebView.

## Trust and reproducibility

Symbolic transforms return `symbolic`, precision `exact`, source conditions and engine identifier. `exact` means coefficients were not numerically rounded; it is not a formal proof. Float64 output returns `approximate` and explicitly no certified error bound. The current release never issues `verified` or `certified_numeric`. The `conjecture` enum is reserved; there is no conjecture-generation endpoint.

History stores the last 256 commands with actor, branch, revision and engine, while the file stores full current branch contents. It is a bounded audit, not a complete event-sourced replay log. Exporting arbitrary source text does not execute shell, Python, Julia or network calls.

## Structured computations

`solve` accepts real single-variable rational-coefficient polynomials of degree at most two. It returns a `Solutions` node, including an empty set when there are no real roots. Infinite solution sets and complex solving are explicit unsupported cases. Matrix shapes are checked before commit; matrix size is limited to 8×8. Inverse and determinants above 2×2 require exact rational entries. Integral nodes retain the body, bound variable and paired optional limits. Finite aggregate nodes retain an integer range and sum/product flag (at most 256 terms). Unsupported antiderivatives fail without a partial mutation.

Cell views also expose `kind`; inspect nodes include explicit `children` for graph traversal. The desktop-only pure `convert_source(source, from_format, to_format)` command converts through the SEG, refusing representations that would lose semantics. It does not mutate the document.

## Definition references and dependency execution

Expression syntax `@f` and LaTeX syntax `\cell{f}` construct a `reference` SEG node. Its target is the authored definition of cell `f` in the active branch. Resolution occurs before domain-sensitive algebra, preserving surrounding binder scope. Ordinary symbols never implicitly become references.

Snapshots expose `dependencies` (`{from,to}` edges, provider to consumer), `calculation_order` and per-cell `dependencies`, `dependents`, `resolved_expression` and `resolved_mathml`. Cells persist an optional `calculation` recipe (`operation`, `variable`, `bindings`), `resolved_root`, `calculation_error` and `calculated_revision`.

Definitions automatically attempt their saved recipe, defaulting to Compute for new cells. Source edits recalculate the edited cell and its transitive consumers. Assumption edits conservatively select cells using the changed symbol in their resolved definitions, then their consumers. Independent results are retained. Recalculation occurs within the triggering transaction/revision and does not add separate human/AI audit events. Undo/Redo restores definitions and derived results together.

`math.recalculate` accepts an optional cell ID; null/omitted recalculates the entire active branch. Explicit transforms retain atomic failure behavior. Automatic mathematical failures commit the authored change but clear failed outputs and expose stable error codes. Structural missing-reference/cycle errors (`missing_reference`, `dependency_cycle`) and resource-limit failures reject the whole mutation. Referenced-cell deletion is rejected; renames update reference tokens without rewriting ordinary symbols.

Contextual CLI `math.preview_latex` also accepts optional `id` to preview a replacement source, including its prospective cycle check. Desktop previews use the current branch in an isolated graph without mutating the document. The standalone LaTeX parser has no external cell context.

KOMYAKU import expands reference definitions to bounded self-contained LaTeX before returning its source draft. It does not import saved outputs or trust claims. Unsupported or oversized expansions fail explicitly.

## SEG collection

`{"expected_revision":45,"actor":"human","command":{"method":"math.compact"}}` collects unreachable SEG nodes across all branches as an undoable, revision-checked transaction. No extra command fields are accepted. Snapshots include `graph_stats: {total, live, unused}`; counts include input, resolved-definition and output roots, shared globally across branches. Cell IDs remain stable, while SEG IDs are valid only with the snapshot revision and can be remapped by collection. Inspect again after a mutation rather than persisting SEG IDs as references.

Automatic collection runs after successful mutations at 2,048 total nodes with at least 256 unused. A resource-limited operation may collect the original valid project and retry once; if it still fails, all changes are rolled back. Pure contextual previews can collect an isolated copy. Undo/Redo retains separate bounded graph snapshots. Desktop manual collection rejects pending drafts, like history navigation.

## Desktop document activation

Desktop-only commands `switch_document {generation,id}` and `close_document {generation}` operate on Rust-owned documents, not mathematical branches. `new_document` and `open_document` retain the previous document. Desktop snapshots add `active_document`, `documents: [{id,name,dirty,path}]`, `generation` and `path`; these fields are not added to CLI math snapshots. IDs identify retained documents within this process. Activation increments `generation` and rejects stale mutation requests even if revisions or persistent project IDs happen to coincide. Closing a modified document requires a successful save; cancellation returns null. The bound is 16 open documents (`document_limit`); Save As to another open document fails with the existing lock error.

## Exact partial substitution

`math.substitute` accepts a cell `id` and a nonempty `values` map of symbol names to exact rational literal strings, for example `{"method":"math.substitute","id":1,"values":{"x":"2"}}` (place the command in the normal revision/actor request envelope). It substitutes into the resolved source before simplification. Unspecified variables remain symbolic; assumption violations, singular values and dimensioned quantity substitutions fail atomically. Reduce calculus operators before substituting their bound/differentiation variable.

Calculation recipes store optional `exact_bindings` separately from float64 `bindings` and replay on dependency changes. Cell snapshots and previews expose `parameters` and `dependent_symbol`; cell snapshots also contain `definition_mathml`. These are presentation metadata, not changes to algebraic free-variable semantics. See [Mathematical contract](mathematical-contract.md).

## Read-only branch comparison

In JSONL `serve`, send `{"method":"math.compare_branches","base":"main","target":"trial"}` outside the mutation envelope. The response contains `revision`, `base`, `target`, `cells`, `assumptions` and `unchanged_cells`. This request rejects unknown fields and missing branch names; it neither saves nor switches branches.

Cells are matched by document cell ID. Each changed entry has `status` (`added`, `removed`, `changed`), `fields`, and nullable `base`/`target` views with typeset definitions, resolved expressions, result evidence, calculation recipes, source and notes. Assumption differences preserve absent versus explicitly configured values. The comparison is structural, not an algebraic equivalence check. Result evidence includes domain conditions, engine and precision; revision-only calculation timestamps are excluded. Node IDs remain document/revision-local.

The desktop `compare_branches` read command additionally requires document generation and revision; stale requests are rejected. Its dedicated comparison WebView can read snapshots/layout and comparisons but is not authorized to edit the document or write files. The view refreshes when the document changes, including document switches. Unapplied drafts are intentionally excluded.

The read request has its own [JSON Schema](comparison-request.schema.json); it is not a mutation request.
