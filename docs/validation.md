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

## Native 3D panel addition · 2026-10-23

- `cargo test -p symbio-core -p symbio-desktop`: **63 passed** (38 core, 25 desktop). Includes 3D sampling, undefined-point masks, explicit z equations, assumption domains, centered finite orbit cameras, v3 dock migration and three-to-four floating-host preference migration.
- `cargo clippy -p symbio-desktop --all-targets -- -D warnings`: passed. Cargo still reports the pre-existing future-compatibility notice for `block 0.1.6`.
- `node --test scripts/*.test.mjs`: **13 passed**; frontend module syntax check passed. `python3 scripts/build-dev-app.py` produced the current unsigned development app.
- Native Tauri / Metal QA, Japanese UI at 1280×800: existing v3 workspace opens with 3D beside Notebook; add `sin(x) cos(y)` from the 3D panel; orbit by pointer drag; edit the LaTeX cell to `z=x^2-y^2` and observe automatic mesh / original height-bound updates.
- Removing x's nonzero assumption restores the missing x=0 sample row (4160 → 4225 valid samples). Changing the common x/y range from [-5,5] to [-2,2] changes z bounds from [-25,25] to [-4,4]. Unsupported `t+x` clears the old mesh and displays a localized variable error; restoring the equation restores the mesh.
- Float 3D into its native host and return it to the dock; verify correct region rendering in both hosts. Save and relaunch the current app: authored equation, existing calculations and dock placement restore, while camera/range initialize as documented session view state.
- This is native application QA using accessibility state and screenshots; no browser console or other operating-system GPU backend was tested. EN/JA/简体中文 strings are provided; this addition's rendered interaction checks used Japanese. Windows/Linux, GPU loss recovery and complex / implicit / parametric surfaces remain untested or unsupported as described in README.

## Startup host-capacity crash correction · 2026-10-23

The reported 15:14 development build aborted during AppKit launch because floating-window preferences still held three slots while the new 3D panel restored slot four. The corrected build migrates previous preference files and derives storage capacity from `Panel::ALL.len()`. Restore/capture also use checked access. Regression tests cover every host slot, old settings with actual saved geometry, and a write/read roundtrip. `cargo test -p symbio-desktop` passed all 27 tests; Clippy passed and the app bundle was rebuilt. A native quit/relaunch restored the saved document and rendered the 3D panel successfully without clearing user settings.

## Three-column default workspace

The default and Reset workspace now use Objects / wide Notebook–3D tab group / Structure & history. Notebook is initially active. Older v1–v3 dock layouts add 3D as a Notebook tab; existing v4 custom layouts remain until reset. Native Japanese UI checks at 1280×800 confirmed the reset layout, central tab switching, native GPU rendering and cell-to-3D automatic tab activation. The desktop's 27 tests and Clippy passed; the development app was rebuilt and the requested default applied to the current workspace.

## 3D axis rulers and independent domains

- `cargo test -p symbio-desktop`: **35 passed**. New coverage checks independent numeric bounds, atomic rejection of invalid second-axis bounds, readable and distinct ticks across narrow / large-offset ranges, constant-height ticks, WebGPU perspective division / screen-y direction, and identical float32 camera serialization in view and label payloads.
- `cargo clippy -p symbio-desktop --all-targets -- -D warnings`: passed. `node --test scripts/*.test.mjs`: **13 passed**. Frontend syntax checks and the unsigned development app build passed.
- Native macOS / Metal, Japanese UI at 1280×800: all three axis names and numeric ticks rendered in the central 3D tab. Set x=[-2,2], y=[-1,3] for z=x²−y²: original z bounds updated to [-9,4], with different x/y ruler values. Pointer orbit moved the rulers and text with the surface.
- A reversed y range was rejected while x/y bounds, height bounds and the retained mesh remained unchanged. The localized error appeared; applying valid bounds restored the normal status. Labels remained present across both error and normal UI rebuilds.
- Text is screen-facing and overlap-filtered; some ticks / names are intentionally hidden when crowded or outside the viewport at extreme views. Axis text uses original values over normalized drawing coordinates. Camera/range settings remain session view state as documented. Native checks for this addition used Japanese; other OS backends and other locale layouts were not separately exercised.

## SEG collection and dependency-graph editing · 2026-10-23

- `cargo test -p symbio-core -p symbio-desktop`: **77 passed** (42 core, 35 desktop). Collection coverage includes all branches, structured expressions and outputs, root remapping, Undo/Redo, save/reload, threshold collection, full-arena retry, isolated previews and atomic rollback. The compact API rejects unexpected fields.
- `cargo clippy -p symbio-core -p symbio-desktop --all-targets -- -D warnings`: passed, with the existing Cargo future-compatibility notice for `block 0.1.6`. `node --test scripts/*.test.mjs`: **15 passed**. Both CLI smoke scripts, frontend syntax checks and the development app rebuild passed.
- Native macOS / Metal, Japanese UI at 1280×800: edit a graph node from a=4 to a=6; dependent b=a+3 and c=a*b recalculate to 9 and 54. The dedicated graph window displays the diagram and selected-cell editor side by side. A self-reference is rejected without changing the document revision.
- A pending graph edit blocks native window closure; Discard restores the saved expression and releases the draft guard. The math keyboard wraps a selected cell-reference expression in a fraction and reports its incomplete denominator in the live preview. Cross-view definition conflict detection is unit-tested; a simultaneous native two-view conflict was not separately exercised.
- Manual collection reduced the existing workspace from 97 nodes before editing to 60 live nodes after editing and collection, preserving every branch and c=54. Save and a fresh-process relaunch restored the collected graph, active branch and calculations. The compact control disables when no unused nodes remain.
- Direct editing of SEG subexpressions or graph edges remains future work. GUI File Open / Save As is the next roadmap priority. This milestone's native layout checks used Japanese; other locales and operating systems were not separately exercised.

## Native Open / Save As · 2026-10-25

- `cargo test -p symbio-core -p symbio-desktop --offline`: **83 passed** (42 core, 41 desktop). File-session tests cover Save As source preservation, continued saving at the new location, retained Undo, lock/write failure rollback, canonical same-path saving, external modification detection, invalid/missing open targets, lower-revision replacement and serialized dialog guards.
- `node --test scripts/*.test.mjs`: **17 passed**, including cross-document snapshot ordering. JavaScript syntax checks, Clippy with warnings denied, both real CLI smoke scripts and the unsigned macOS development bundle build passed. Cargo retains the existing future-compatibility notice for `block 0.1.6`.
- Native macOS / Metal at 1280×800, Japanese UI: cancel Save As and retain the original location; save a separate copy and display its active path; open another document at revision 0 from revision 45 and update the objects, dependency graph and 3D surface. Save As to `symbio-saved-qa.symbio` retained the authored document and selected the new destination.
- Rename the temporary document without saving, open the original file, and cancel the save confirmation: the draft document name, dirty state and active path remain. Repeat with Save and Open: the temporary document is saved before switching. Disk inspection confirmed the saved name and the unchanged original workspace revision.
- Quit and relaunch the current bundle with a nondefault active document: its file path, name, revision and rendered surface restore. Finally reopened the user's original workspace, saved at revision 45. Screenshots confirmed the compact toolbar and three-column layout.
- File dialogs and confirmation text were exercised in Japanese; EN/简体中文 strings are supplied but their native layouts were not separately tested. Windows/Linux dialogs, forced disk-full failures and two-process native lock interaction remain untested; lock failure and external-edit preservation are covered by Rust tests. Simultaneously retained documents remain future work.

## New blank document · 2026-10-25

- Rust tests: **85 passed** (42 core, 43 desktop). Added coverage for a genuinely empty project, release of the previous file lock, no inherited history/assumptions/results, refusal to overwrite the original through Save, blank-file roundtrip, and calculation/persistence of the first equation at cell ID 1. Clippy passed; the development app was rebuilt.
- Native Japanese UI at 1280×800: the new-document icon is immediately left of Open. Click it to get an untitled notebook with 0 SEG nodes, no cells, assumptions or history, and an Unsaved indicator. The 3D panel clears its previous samples and shows the empty-state prompt.
- First Save opens a location picker; cancelling retains the untitled document. Add the first equation, then New → Save and Continue → cancel location selection: the equation and dirty document remain. Saving to a temporary `.symbio` file succeeds. The original workspace was reopened unchanged at revision 45 before the final blank-document visual check.
- English/Japanese/Simplified Chinese strings are supplied. Native checks used Japanese/macOS; other OS and locale layouts were not separately exercised. Untitled documents are memory-only until first save.

## Multiple retained documents · 2026-10-25

- `cargo test -p symbio-core -p symbio-desktop --offline`: **89 passed** (42 core, 47 desktop). New coverage exercises inactive unsaved changes, independent Undo/Redo, retained file locks, duplicate-path lookup, file unlock on close, closing the last document, failed switch/close rollback, the 16-document bound and Save As protection for another retained file.
- Clippy with warnings denied, frontend syntax checks, **17 JavaScript tests**, and the unsigned development bundle build passed. The existing `block 0.1.6` future-compatibility notice remains.
- Native Japanese/macOS at 1280×800: keep the original workspace and two untitled documents open; add e1=x² to document 2; activate empty document 3 without saving; switch back to document 2 and recover its equation and enabled Undo. Undo clears only that document; Redo restores e1. The picker lists all three documents and marks modified / untitled state.
- Native testing exposed that the predefined AppKit Quit menu bypassed Tauri's `ExitRequested` event. The replacement menu command now routes through `AppHandle::exit`. Repeating ⌘Q while empty document 3 is active and modified document 2 is inactive leaves the app running and displays the save-before-exit message. This validates the custom Quit menu, not forced OS termination.
- Close modified document 2 → Cancel preserves its equation and document entry. Save it to a temporary file, close it, then close empty document 3: the original workspace remains at revision 45 with its calculations. A screenshot confirmed the compact title/picker/close control fits beside the existing toolbar actions.
- The document set is retained only in memory; full session recovery and Save All remain future work. Camera/range/layout remain shared view settings. Other operating systems, other locale layouts and long-running 16-document workloads were not exercised natively.

## Research reading flow and progressive controls · 2026-10-25

- Reviewed the existing native workspace against definition → assumptions → operation → result → dependencies. The implemented decisions are documented in `ui-review.md` and the root `design.md`. No participant-based usability claim is made.
- Saved cells now lead with typeset input/result; source editing opens on demand and for new cells. Native Japanese/macOS QA at 1280×800 verified y differentiation of x²−y² (displayed partial-derivative control), then explicit x=2, y=3 numeric fields → **−5**, with bindings and `float64 · no certified error bound` retained. No JSON entry is required.
- Native source editing: enter incomplete `\\frac{` in an existing cell, observe the parser error and suppression of its previous output, then Discard → original reference expression and result **9** restore at the same revision. Successful operations return to the reading view.
- Object selection from the 3D tab activates Notebook, reveals the selected cell and updates the inspector. Uses / Used by links navigate between related cells; an expanded dependency disclosure stays open across selection and inspection updates. SEG internals and audit history remain available in disclosures. A narrowly scoped `reveal_notebook` command preserves the existing docking authorization boundary.
- Inspected the docked notebook at several narrow widths around 320–420 logical pixels, the normal wide central pane, and its 800-pixel floating window. Controls wrapped and results stayed readable. Exact 320/375/414/768 browser viewport certification was not performed: this is a native desktop application with child WebViews and a GPU surface, so native QA was used. Reset restored the default dock layout after these checks.
- `cargo test -p symbio-core -p symbio-desktop --offline`: **89 passed**; Clippy with warnings denied passed; **17 JavaScript tests** passed; frontend syntax checks and the unsigned development bundle build passed. The existing `block 0.1.6` future-compatibility notice remains. Core mathematical algorithms and file formats were unchanged.
- The original workspace's mathematical definitions/results were retained; navigation checks ended with b selected, saved at revision 55. Temporary mathematical experiments used a separate document. Native checks used Japanese/macOS; English and Simplified Chinese labels were supplied but their rendered layouts were not separately exercised.

Screenshot: [research workspace](screenshots/research-workspace-ja.jpg).

## Screenshot feedback adoption (2026-10-25)

- Rust: **98 tests passed** (51 core, 47 desktop); Clippy with warnings denied and the development app build passed.
- JavaScript: **17 tests passed**. Optional local SymPy 1.14.0 oracle: **6 bounded comparisons passed**. This does not establish broad CAS conformance.
- Native macOS checks: partial exact substitution with x=2 and blank y; numeric x=2,y=3 retaining output z; Undo restores the exact recipe; save/reload retains it; explicit model roles and Unspecified y are visible. Source Apply works with empty hidden numeric fields after separating form validation.
- Advanced physics acceptance suites, formal proof, arbitrary precision floats, interval arithmetic and publication-quality figure export remain unimplemented or untested.

Final native reload confirmed the typeset exact result `z = 4-y^2`. The original saved research document was restored at revision 55 without modifying its mathematics. CLI and math-keyboard end-to-end smoke checks also passed.
