# AI research

## Desktop notebook

Open **AI Research** next to the handwriting button in the notebook. Choose a provider, enter a tool-capable model ID, and type a question. The **+** button copies the selected applied formula into the question for review before sending.

| Provider | Model endpoint | Credential |
| --- | --- | --- |
| OpenAI | OpenAI Chat Completions | API key field or `OPENAI_API_KEY` |
| Google Gemini | Gemini API OpenAI-compatible Chat Completions | API key field or `GEMINI_API_KEY` |
| Local server | Chat Completions-compatible HTTP(S) on loopback, such as `http://127.0.0.1:11434/v1` | Optional API key field or `SYMBIO_LOCAL_API_KEY` |

Enter the base URL up to the API prefix. Symbio appends `/chat/completions`. Use a model and server that support function calls and tool responses. Point Symbio at the local server's host and port before submitting, and start that server separately. See [OpenAI Chat Completions](https://platform.openai.com/docs/api-reference/chat/create) and [Gemini API compatibility](https://ai.google.dev/gemini-api/docs/openai).

Use **Notebook context** to select applied formulas and assumptions individually. Review the exact serialized context before sending. The selector starts empty and shares the selected original sources and assumptions. Notes, calculation history, and referenced cell definitions are outside this selection unless you select those formulas individually. Reopen AI Research if the notebook changes while you are selecting context.

Submitting sends the question and any subsequent tool results to the selected provider. The two available tools run in scratch Rust state. The dialog shows the provider's response and a separate record of the requested operations and kernel results. Open a successful evidence entry to preview the original formula and its scratch assumptions. **Add formula to notebook** creates a new AI-attributed cell after explicit approval. The notebook applies its current assumptions and recalculates through the normal kernel. If the notebook has changed since the question was submitted, run the question again. Keys entered in the dialog are discarded when it closes. Provider data handling and charges depend on your account and service.

If the proposal has scratch assumptions, **Add formula and assumptions to new branch** copies the current branch and applies the listed assumptions in the copy. An assumption with the same symbol replaces the inherited value there. The parent branch retains its original assumptions and results. The new definition and the recalculation of affected cells are recorded as one AI-attributed revision and can be undone together.

Use **Cancel research**, or close the dialog, to abort the local wait and stop subsequent tool calls and provider requests. A scratch calculation already executing may finish in the background. The provider may continue processing a request it has already received. The question remains available for retry when the dialog stays open.

日本語：ノートの「AI研究」で接続先とモデルIDを選び、質問を送信します。「＋」で選択中の数式を質問に追加できます。回答とRustの計算根拠は別々に表示されます。

简体中文：在笔记中打开“AI 研究”，选择服务并填写模型 ID，然后发送问题。可使用“＋”将所选公式添加到问题中。模型回答与 Rust 计算依据分别显示。

## Symbio CLI

```sh
cargo build --locked -p symbio-cli
./target/debug/symbio-cli research openai "$OPENAI_MODEL" < question.txt
./target/debug/symbio-cli research gemini "$GEMINI_MODEL" < question.txt
./target/debug/symbio-cli research local "$LOCAL_MODEL" http://127.0.0.1:11434/v1 < question.txt
```

Set the applicable API key in the environment. Output is JSON with `answer` and `evidence`. Each run handles one question with up to five provider requests and eight tool calls. Requests time out individually. Each provider response is limited to 1 MiB.

## Codex and standalone Antigravity

Build `symbio-cli` and use its absolute path in the agent's MCP configuration. The MCP process uses the agent's existing model session. Symbio provider credentials are unnecessary for this route.

For Codex, register the stdio tool from the CLI and start a new session:

```sh
codex mcp add symbio -- /absolute/path/to/Symbio/target/debug/symbio-cli mcp
codex mcp list
```

For **Antigravity Desktop 2.0**, open **Settings → Customizations → Installed MCP Servers → Open MCP Config**. For **Antigravity CLI** (`agy`), use the project-level `.agents/mcp_config.json`. Add the `symbio` entry to the existing `mcpServers` object.

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

Refresh installed MCP servers in Antigravity Desktop. In the CLI, start a new session and check `/mcp`. See [Google's Antigravity configuration](https://developers.google.com/knowledge/mcp?tab=google-antigravity).

Example request for either agent:

> Use Symbio to integrate x² from 0 to 2. Report the exact result and any kernel conditions.

The agent can use `math_calculate` and `math_preview_latex` for individual calculations. See the [MCP tool contract](mcp.md) for inputs, returned evidence and limits. Use `math_calculate` with `operation: "substitute"` and string-valued `exact_bindings` for exact rational substitution. Each tool call uses scratch state. Include any relevant formulas and assumptions in the prompt.

For explicit read-only access to a saved branch, register a separate MCP entry with arguments `["mcp", "--read-project", "/absolute/path/to/study.symbio"]`. Append `"--branch", "research-1"` to the argument list to select a specific saved branch. See [the notebook access contract](mcp.md#optional-notebook-access).
