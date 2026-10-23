# Symbio · Symbolic Intelligence Workbench

<img src="docs/images/symbio-image.png">

Symbio is an early mathematical workbench built with Tauri 2 and Rust. It keeps mathematical meaning, assumptions and research history together, with the same Rust kernel serving both the desktop UI and CLI. It reuses ThreeX's wgpu renderer and ATOM's native panel infrastructure.

**The current release is a Semantic Expression Graph (SEG) MVP. Mathematica compatibility, advanced CAS algorithms, LLM integration and formal proof engines are not implemented.**

![Symbio desktop](docs/screenshots/workspace-ja.png)

## Build and run

Generate the unsigned macOS development app locally. Rust/Cargo, Xcode Command Line Tools and Python 3 are required. No Node/npm frontend build is needed.

```sh
python3 scripts/build-dev-app.py
open target/debug/Symbio.app
```

To open a specific document, create its parent directory first, then run:

```sh
cargo run --locked -p symbio-desktop -- --project /absolute/path/study.symbio
```

If the document does not exist, Symbio creates polynomial and `sqrt(x^2)` examples. Without a project argument, it uses `workspace.symbio` in Tauri's application data directory, normally `~/Library/Application Support/dev.symbio.workbench/` on macOS. Dock layouts are stored in `dock-layout.json` in the application configuration directory. Set `SYMBIO_LAYOUT_PATH` to isolate layout persistence during testing.

The toolbar supports English, Japanese and Simplified Chinese. Expression mode requires explicit multiplication (`2*x`); LaTeX mode also accepts `2x` and previews formulas while editing. Use the checkmark to apply edits, operation buttons to compute, and **⌘S** to save. Windows and Linux builds have not been tested.

## Using the workspace

- **Left:** Add and select expressions, configure variable domains, nonzero conditions and dimensions `[M,L,T]`. Branches let you edit expressions and assumptions independently.
- **Center:** Edit source, view typeset formulas and results, and write research notes. Operations include simplification, differentiation, supported definite and indefinite integrals, matrix operations, equations up to degree two and float64 evaluation. **⌘Enter** computes the current expression. Enter numeric bindings as JSON, such as `{"x":2}`.
- **Right:** Inspect SEG nodes referenced by inputs and outputs, together with operation history. History records the human/AI actor, branch, revision and engine. Example results are also computed in Rust.
- Drag panel tabs to join a tab group at the center, split along an edge, or float outside the workspace as a native window. The placement menu offers the same operations. Closing a floating window returns its panel to the dock.
- Resize splitters by dragging or using arrow keys. Language, layout and window placement are persisted.
- The plot icon opens a separate wgpu window. Rust evaluates the expression at 1,024 points over `x ∈ [-5,5]`, retaining samples and rendering data in Rust/GPU memory. Only the variable `x` is supported. Undefined samples are omitted; error bounds and continuity are not certified.
- The JSON icon displays the complete document for inspection and selection. Choose a document path through the CLI or startup arguments.

Saving applies pending expression and note edits. Syntax errors cancel the save, and closing with unsaved changes is blocked. Apply pending edits before switching branches, undoing, redoing or deleting cells. Document Undo/Redo covers the most recent 32 operations in the current session.

## Math keyboard

Open the keyboard icon below an expression. Templates can wrap selected source in fractions, powers, roots or absolute values and select the next argument. The keyboard includes numbers, operators, elementary functions, cursor movement, deletion and input-specific Undo/Redo. In LaTeX mode, the calculus and matrix tabs insert directly computable templates. Use **Tab / Shift+Tab** to move between `□` placeholders.

The research-note keyboard also preserves templates from the supplied `mathkeyboard.html` in its **Notation** tab, including fractions, integrals, limits, sums and sets. The computational palette contains supported syntax; unsupported reference notation produces an explicit error. Fill every `□` before computing.

Select **LaTeX** input mode to parse supported notation into SEG in Rust and preview it as MathML. Apply the source with the checkmark, or directly invoke a mathematical operation. In a research note, use the preview icon to render LaTeX and **+** to create a separate computational cell while retaining the note. No connection to the original website or MathJax is required. Input Undo/Redo is independent of document history and retains the most recent 32 edits.

### Supported LaTeX

Switching source formats converts through the Rust semantic representation. For example, `sqrt(x^2)` becomes LaTeX with its square-root meaning preserved. Conversion rejects structures that Expression mode cannot represent, such as integral and matrix nodes, and retains the original input.

Example: `\frac{x^{3}+2x^{2}-5x}{2}` → formula preview → differentiation → `3/2*x^2 + 2*x - 5/2`.

- Arithmetic, fractions `\frac{a}{b}`, roots `\sqrt{x}`, powers `x^{2}`, implicit multiplication `2x`, parentheses, brackets and braces.
- `\sin(x)`, `\cos(x)`, `\exp(x)`, `\ln(x)`, `\log(x)` (natural logarithm), `\sin^{2}(x)` and `\left|x\right|`. Function arguments must be grouped.
- `$...$`, `$$...$$`, `\(...\)`, `\[...\]`, `\left(...\right)` and LaTeX spacing commands.
- Supported Greek letters map to variables named `alpha`, `beta`, `gamma`, `delta`, `theta`, `lambda`, `mu`, `sigma` and `omega`. For example, bind `\theta` using `{"theta":2}`. `xy` means `x*y`. `\pi` is the circle constant, and `x_1` is a subscripted variable.
- Definite integrals such as `\int_{0}^{1}{x^{2}}\,dx` → `1/3`. Indefinite integrals such as `\int{x^{2}}\,dx` return an antiderivative with an arbitrary-constant condition. Supported integrands include polynomials, constants and selected sin/cos/exp expressions. General rational-function and improper integrals are unsupported.
- Derivative notation `\frac{d}{dx}(x^{3})` and `\frac{\partial}{\partial x}(x^{3})`; finite integer sums `\sum_{i=1}^{5}{i}` and products `\prod_{i=1}^{5}{i}`, with at most 256 terms.
- Matrices such as `\begin{bmatrix}1 & 2 \\ 3 & 4\end{bmatrix}`. Supported environments are matrix/pmatrix/bmatrix/Bmatrix/vmatrix. Operations include addition, subtraction, multiplication, scalar scaling/division, transpose, integer powers, determinant and inverse. Matrices are limited to 8×8 and powers to −16…16. Inverse and determinants of matrices larger than 2×2 require rational entries.
- Equations such as `x^{2}-5x+6=0` remain SEG relations. Select **x = ?** to obtain `{2,3}`. Solving supports rational-coefficient, single-variable polynomials up to degree two over the reals. Identities, complex solutions and general simultaneous equations are unsupported.
- Limits, arbitrary TeX macros, multiple-integral symbols and higher-order derivative notation are unsupported.
- Fill all `□` placeholders before computing. Group multi-character exponents, for example `x^{12}`. Each source is limited to 2,048 UTF-8 bytes, 512 tokens and nesting depth 64. Preview does not mutate the document or certify mathematical definedness or correctness.

MathML typesets fractions, integral limits, matrices and equations. The formula focus view provides adjustable display size. Long formulas scroll horizontally instead of breaking across lines.

## KOMYAKU integration

The neighboring KOMYAKU project uses the same `symbio-core` from its math panel for local computation. Authored LaTeX remains in KOMYAKU's Canonical/Yjs history; SEG and calculation results are derived data.

KOMYAKU's **↗ .symbio** exports the current source and its explicit assumptions. Open the resulting document in Symbio with:

```sh
cargo run --locked -p symbio-desktop -- --project /absolute/path/equation.symbio
```

KOMYAKU's **↙ .symbio** reads the selected equation from the active branch of a saved Symbio document into a reviewable LaTeX draft. Rust validates the file and converts Expression syntax when necessary. Stored calculation results are not adopted. This integration exchanges individual equations; full document-history synchronization and `.komyaku` Archive import/export are outside its scope.

## Implementation boundaries

- SEG is an immutable, structurally shared DAG. Structural identity is distinct from mathematical equivalence.
- Integers, rationals and finite decimal input are exact. Arbitrary-precision floating point, complex numeric evaluation and certified interval arithmetic are unavailable.
- `sqrt(x^2)` simplifies to `abs(x)` for real `x`, and to `x` for nonnegative or positive `x`. It remains unchanged for complex or unspecified domains. `x/x` is not cancelled without a nonzero assumption.
- Rational-coefficient, single-variable polynomials up to degree 32 support expansion, differentiation and integration. Differentiation also covers selected elementary functions. General integration, factorization and higher-degree equation solving are unsupported.
- Changing assumptions clears results in the current branch. Recompute explicitly using operation buttons. Cell names are display labels and do not yet resolve references between cells.
- Results are `symbolic` or `approximate`. `conjecture` is a reserved data type. The release does not issue `verified` or `certified_numeric` results or claim formal proofs or certified numerical errors.
- `.symbio` v1 is readable JSON. SQLite containers, content-addressed data, complete event replay and branch merging remain future work. The audit retains 256 events; Undo stacks are not persisted.
- The structured API accepts an AI actor attribution, but AI connections, natural-language interpretation and an MCP server are not implemented. The app makes no external network requests.

[Architecture](docs/architecture.md) · [Math API](docs/math-api.md) · [Roadmap](docs/roadmap.md) · [Provenance](docs/provenance.md) · [Validation](docs/validation.md)

## Development checks

```sh
cargo test --locked -p symbio-core -p symbio-cli -p symbio-desktop
cargo clippy --locked -p symbio-core -p symbio-cli -p symbio-desktop --all-targets -- -D warnings
python3 scripts/smoke-cli.py
python3 scripts/smoke-math-keyboard.py
node --check apps/desktop/ui/app.js
node --check apps/desktop/ui/dock.js
node --test scripts/math-keyboard.test.mjs scripts/latex-preview.test.mjs
```

Serving `apps/desktop/ui` through a static server provides a **read-only** preview of sample data generated by Rust. The browser does not contain an alternative JavaScript calculation engine.
