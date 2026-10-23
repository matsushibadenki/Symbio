# Validation · 2026-10-23

Environment: macOS / Apple Silicon, local development build. Verification used the real Tauri app through native UI automation, plus Rust and CLI tests. Browser-only preview is explicitly read-only.

## Automated

- `cargo test --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop`: **35 passed** (17 mathematical/LaTeX/storage integration tests, 18 desktop/layout/placement/plot tests).
- `cargo clippy --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop --all-targets -- -D warnings`: passed.
- `cargo fmt --all --check`: passed.
- `node --check apps/desktop/ui/app.js` and `dock.js`: passed.
- `python3 scripts/smoke-cli.py`: real CLI initialization, JSONL requests, AI branch/assumption attribution, simplify, explicit save, fresh-process reload, stale revision and malformed expression rollback passed.
- `python3 scripts/build-dev-app.py`: built the unsigned macOS `.app` successfully.

Rust cases cover structural sharing, exact decimal/rational and big integer arithmetic, operator precedence, derivative/integral evaluation, safe conditional simplification, preservation of undefined domains, revision conflicts, undo audit, branch assumption isolation, numeric trust, dimensional mismatches/extreme values, invalid DAG/parser limits, locking/external changes and save roundtrip. Desktop cases cover split partitions, tab/floating-group transitions, stale commands, layout migrations, monitor bounds/DPI math and visible scene creation.

The dependency `block 0.1.6` produces Cargo's future-incompatibility notice. Current builds and checks pass; upgrading the upstream native dependency chain is a future maintenance task.

## Native UI

Verified through actual controls:

- Initial three-pane load; corrected a snapshot/layout startup ordering race.
- Edit `f` to `x^4+2*x`, differentiate, see `4*x^3+2`.
- Edit to `sin(x)` and save from toolbar while the textarea is still a draft; restart and recover the edited source.
- Branch `positive-x`, change `x` from real to positive, observe prior results invalidated, simplify `sqrt(x^2)` to `x`.
- Float the inspector into its native window and return it to the dock.
- Resize the nested splitter using the keyboard; its percentage changes from 70 to 72.
- Switch Japanese → Simplified Chinese → English → Japanese across all panels.
- Evaluate the sample polynomial at `x=2`, see `6` with `Approximate` and `float64 · no certified error bound`.
- Render the polynomial in a native wgpu window. Fixed Metal surface creation to occur on the UI thread and corrected ThreeX create/update command sequencing.
- Browser preview loads without captured console errors.

Screenshots: [Japanese workspace](screenshots/workspace-ja.png), [English numeric evaluation](screenshots/workspace-en.png), [Simplified Chinese](screenshots/workspace-zh.png), [native GPU plot](screenshots/native-plot.png).

## Not claimed

No Windows/Linux execution, distribution signing/notarization, GPU device-loss test, physical multi-monitor unplug test, full cross-platform accessibility certification, broad CAS conformance suite or long-duration performance benchmark was performed. Proof/LLM/FLINT/Julia engines are not present, so no integration success is claimed for them. See the roadmap for explicit pending and future work.

## Initial math keyboard addition

- `node --test scripts/math-keyboard.test.mjs`: **7 passed**. Covers selected numerator wrapping, denominator selection/replacement, roots/powers, original LaTeX snippets and placeholder focus, literal replacement characters, Unicode deletion, bounded undo/redo and redo-branch truncation.
- JavaScript syntax checks and macOS `.app` rebuild passed; mathematical Rust code is unchanged.
- Native Tauri UI: select `x+1` → fraction → denominator `2` → input Undo/Redo → Evaluate at `x=2` → **1.5**. The result is from the Rust kernel.
- Native Tauri UI: research note → LaTeX keyboard → calculus → integral → selected lower-bound placeholder → enter bounds and integrand → toolbar Save → saved `\int_{0}^{1} x^2 \, dx` as a note.
- UI labels added in English, Japanese and Simplified Chinese; this addition's native interaction test used Japanese at 1280×800. Other locales were not separately re-tested for this addition.
- [Expression keyboard](screenshots/math-keyboard.png), [LaTeX note keyboard](screenshots/math-keyboard-latex.png). Actual native app screenshots; the layout remains scrollable with the keyboard open.

## LaTeX → SEG addition

- `cargo test --workspace --locked --offline`: **35 Symbio tests passed** (17 core, 18 desktop), plus 83 vendored ThreeX tests passed; 11 vendor benchmarks/GPU tests remain ignored.
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: passed.
- `node --test scripts/math-keyboard.test.mjs scripts/latex-preview.test.mjs`: **10 passed**. Includes outdated async preview suppression, clearing invalid previews, detached/empty preview handling and selected compound LaTeX power grouping.
- Rust cases compare LaTeX and expression roots for identical SEG sharing; verify fraction/root MathML, rational simplification, polynomial derivative/integral, approximate evaluation, assumptions, unsupported syntax/macros, bounded parsing, transaction rollback, file roundtrip, legacy format defaults, source-format tampering and Undo/Redo.
- CLI smoke covers `math.preview_latex` → `math.define_latex` → differentiate → explicit save → fresh-process reload.
- Native UI: source format → LaTeX; enter `\frac{x^{3}+2x^{2}-5x}{2}`; live fraction/power preview; Differentiate → `3/2*x^2 + 2*x - 5/2`; inspector displays the same SEG. Incomplete `\frac{x}{` clears the preview and hides previous output without changing the document revision.
- Native UI: unsupported `\int` note shows an explicit error. Replace with `\frac{1}{3}+\frac{1}{6}` → preview → + → separate LaTeX cell → Simplify → exact `1/2` → Save. The note and computational cell are both retained.
- Native restart recovered LaTeX source formats, original sources, notes, derivative output and exact `1/2` output from disk.
- [Native LaTeX calculation](screenshots/latex-seg-ja.png). UI text is translated in EN/JA/简体中文; the initial end-to-end interaction ran in English and the screenshot uses Japanese.

Supported mathematical LaTeX is a documented subset, not a full TeX renderer or CAS importer. Preview checks syntax, not domain validity. Unimplemented integral/sum/matrix/equation notation remains an explicit error.
