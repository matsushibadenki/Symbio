# Local MCP adapter

Symbio exposes a bounded, stateless math-tool server through `symbio-cli mcp`. It uses the same Rust SEG, LaTeX parser, assumptions and computation engine as the notebook. It does not attach to a running desktop app, open a project, edit a document or retain research state between calls.

## Start

```sh
cargo build --locked --offline -p symbio-cli
./target/debug/symbio-cli mcp
```

Configure an MCP client to launch the absolute path of the built executable with `args: ["mcp"]`. Do not wrap it in the old `serve PROJECT` protocol. A generic client configuration entry looks like:

```json
{
  "mcpServers": {
    "symbio": {
      "command": "/absolute/path/to/Symbio/target/debug/symbio-cli",
      "args": ["mcp"]
    }
  }
}
```

No client configuration is installed or changed automatically. Package the release executable for distribution rather than relying on a local development path.

## Tools

The bundled [tool definitions](mcp-tools.json) contain the JSON Schemas served by `tools/list`.

| Tool | Purpose | Structured result |
| --- | --- | --- |
| `math_calculate` | Calculate or transform one Expression/LaTeX source in a fresh scratch SEG | `ok`, engine, notebook cell with authored/result MathML, evidence, precision, conditions, trust, and reachable SEG inspection |
| `math_preview_latex` | Parse mathematical LaTeX and produce a preview | `ok`, engine, expression text, MathML, kind, symbols and free parameters |

All tools declare read-only, non-destructive, idempotent and closed-world annotations. These describe the tools' behavior, not an authorization mechanism. Each calculation constructs its own Rust AppCore and uses normal transactions attributed to AI; no shared document or history is mutated. Node IDs have meaning only within that call's returned graph.

For example, call `math_calculate` with:

```json
{
  "source": "\\int_0^2 x^2\\,dx",
  "source_format": "latex",
  "operation": "compute"
}
```

The cell result is exact `8/3` with Symbolic trust. Another example is `{"source":"sqrt(x^2)","assumptions":{"x":{"domain":"real"}}}`, which returns `abs(x)`. A later call without those assumptions does not inherit them. Trust and mathematical conditions come from the kernel; output never claims formal verification.

Supported operations are `compute`, `simplify`, `differentiate`, `integrate`, `expand`, `solve`, `determinant`, `inverse`, `transpose` and `evaluate`, within the existing [mathematical contract](mathematical-contract.md). `variable` defaults to `x`; source format defaults to Expression and operation to `compute`. Numeric `bindings` apply **only** to explicit `evaluate`; other operations reject nonempty bindings. For example, `{"source":"x^2","operation":"evaluate","bindings":{"x":2}}` returns an Approximate result with the kernel's float64 precision warning. Exact substitution is not exposed yet.

Unknown fields and operations, executable paths, project paths and external CAS operations such as `factor` are rejected. Cell references cannot refer to another call or the desktop. Mathematical LaTeX is parsed, not run through TeX. No Python, shell, network, file save, remote model, GPU or installer is invoked by these tools. An inherited `SYMBIO_CAS_PYTHON` does not enable CAS through this adapter.

## Protocol and boundaries

The adapter follows the MCP [stdio transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports), [lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle), and [tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools) interfaces. Send `initialize`, accept the negotiated version, then `notifications/initialized` before tool discovery/calls. Supported protocol versions are `2025-11-25` and `2025-06-18`; other proposals receive the latest supported version, which the client must accept or disconnect. `ping` is also available. There is no pagination, dynamic tool list, resource/prompt capability, HTTP transport or server-initiated sampling.

Each UTF-8 JSON-RPC frame is one line. Stdout contains protocol messages only; startup/transport failures go to stderr. Notifications receive no responses. Invalid envelopes/JSON, unknown methods/tools and invalid argument shapes return protocol errors; kernel parse/domain/unsupported/limit failures return a tool result with `isError: true`, `ok: false` and a stable kernel error code. Successful results include the same data as serialized JSON text and `structuredContent`; both success and tool failure conform to the advertised output schema.

Frames are limited to 64 KiB (oversized input terminates the process); mathematical source is limited to 2048 UTF-8 bytes, and assumptions/bindings to 64 entries. The normal parser/graph/algorithm bounds apply. Serialized responses exceeding 2 MiB return a protocol error. This is a synchronous local server: request cancellation/progress/tasks and hard wall-clock deadlines are not implemented. Unknown notifications are ignored, so client cancellation does not interrupt an in-flight calculation; the client can terminate its subprocess. No arbitrary code is evaluated, but this is not an OS sandbox or a general CAS service.

`python3 scripts/smoke-mcp.py` runs an independent stdio client and validates the advertised JSON Schemas using a locally installed Python `jsonschema` package (test dependency only). Dedicated third-party MCP host integration remains to be checked; protocol tests alone are not full ecosystem certification.

## Next steps

Research-document access, human-reviewable AI proposals, approval-bound changes, natural-language planning, exact substitution and explicit external-CAS permissions remain separate planned work. This server intentionally cannot approve or apply changes to a user's notebook.

日本語：MCPクライアントから同じRustカーネルで独立計算できます。各呼び出しは新しい状態で実行され、開いている研究文書には接続しません。`math_calculate` と `math_preview_latex` のSchemaを参照してください。

简体中文：MCP客户端可调用同一Rust内核进行独立计算。每次调用都使用新状态，不连接或修改已打开的研究文档。请参阅两个工具的JSON Schema。
