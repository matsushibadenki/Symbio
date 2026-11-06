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

## Read-only branch comparison (2026-10-25)

Target flow: Objects branch picker → comparison icon → choose another branch → inspect typeset differences without checkout or mutation.

- Rust: **103 tests passed** (56 core, 47 desktop). Five new tests cover stable identity through rename/add/remove, changed reference dependencies, exact recipes, source notation, assumptions/evidence, notes, errors, collection/reload, reverse comparison and read-only state.
- JavaScript: **17 tests passed**; frontend syntax checks passed. Clippy passed with warnings denied. The existing `block 0.1.6` future compatibility notice remains.
- The comparison WebView has only read access to snapshots/layout/comparison and event listening. Generation/revision guards and superseded-response rejection prevent displaying old-document results.
- Browser skill is not installed. Validation uses the native macOS app because child WebViews, Tauri IPC authorization and separate windows are the actual target; a static browser preview cannot exercise that flow. Native runtime console logs are not available through the UI automation surface.
- Structural equality is not mathematical equivalence. Merge, historical replay, Windows/Linux and long-running multi-monitor behavior remain outside this change.

Native QA completed on macOS, in the 1000×720 comparison window:

| Check | Result |
| --- | --- |
| Window identity and meaningful content | Passed: `view=comparison`, localized title, typeset definitions/results. |
| Comparison target interaction | Passed: main → nonnegative-model → main; differences and unchanged-cell count updated. |
| Assumption-sensitive results | Passed: nonnegative `x` versus real `abs(x)` appeared alongside their recorded conditions. |
| Read-only state | Passed: closing comparison retained active linear-model, saved state and revision 6. CLI also verified identical before/after snapshots and unchanged file bytes. |
| Layout | Passed at 1000×720: two aligned columns, scrollable formulas/details, no visible overlap or error overlay. |
| Runtime console | Not captured by native UI automation; syntax checks, IPC interaction and rendered content passed. |

The first native pass found an always-disabled button caused by serializing `disabled=false` as an HTML boolean attribute. It was corrected to omit the attribute when enabled; the rebuilt app's button and comparison flow passed. The original research document was restored saved at revision 55. Screenshot evidence: `/private/tmp/symbio-branch-comparison-ja.jpg`. Live edits/document switches are protected in implementation; they were not separately exercised while the comparison window remained open. Narrower window sizes, other platforms and extended sessions remain untested.

## Source-based reproduction packages (2026-10-28)

Target flow: Structure & history → Reproduction → native export → select a package → fresh source replay → per-cell evidence report, without modifying the editor document.

- **115 Rust tests passed** (68 core, 47 desktop). Twelve new cases cover references and exact substitution, source/evidence changes, integrity checks, scalar float bits, engine mismatch, expected errors, dimensional failures, missing recipes/expectations, invalid/oversized inputs, collection/reload stability, LaTeX integrals/matrices/equations, legacy sample annotations, no-clobber/symlink behavior and bounded previews for shared DAGs.
- **17 JavaScript tests passed**; syntax checks passed. Clippy with warnings denied passed; the unsigned development app built. Existing dependency `block 0.1.6` still emits its future compatibility notice.
- `python3 scripts/smoke-reproduction.py`: passed export/replay, immutable source bytes, refusal to overwrite, checksum failure and a resealed expected-result mismatch. CLI verification exits nonzero for incomplete/mismatching packages.

| Native macOS check (1280×800) | Result |
| --- | --- |
| Page identity / meaningful content | Passed: inspector reproduction section, localized native Save/Open dialogs and report content. |
| Export | Passed: active graph-edit-demo branch written to a new temporary JSON file; save location and revision 59 retained. |
| Replay match | Passed: sample package returned two matching results, zero differences/skips; expanded result displayed typeset `abs(x)`. |
| Old engine evidence | Passed: the existing workspace's four recorded outputs were flagged as engine differences. Final UI explicitly displayed `symbio-rust/0.4.0 → symbio-rust/0.5.0`. Existing results were not replaced. |
| Layout / error overlay | Passed: compact controls, wrapped path/checksum, expandable rows and MathML remained inside the inspector; no visible error overlay. |
| Original document state | Passed: saved graph-edit-demo at revision 59 before and after verification. |
| Console | Native UI tooling does not expose runtime console logs; syntax checks and actual IPC/file interactions passed. |

Native validation used `cua_repl` because child WebViews and file dialogs require the desktop runtime. The Browser skill is not installed; a static browser preview would not exercise these paths. Evidence images are `/private/tmp/symbio-reproduction-matched-ja.jpg` and `/private/tmp/symbio-reproduction-engine-difference-ja.jpg`.

Not validated here: Windows/Linux, cross-platform float agreement, large/long-running packages, cancellation during costly symbolic work and full environment reconstruction. Full historical event replay and formal proof are not implemented. The package checksum is not an authenticity signature.

## Exact MathJSON subset exchange (2026-10-28)

Target flow: Structure & history → MathJSON → export selected definition / import a JSON expression → new untitled notebook → normal Rust calculation.

- **124 Rust tests passed** (77 core, 47 desktop), including nine new MathJSON cases: exact large integers/rationals, explicit versus absent domains, resolved references and unchanged owner state, matrix product/determinant after roundtrip, quadratic equation solving, object forms/constants/functions, rejected metadata/numbers/unsupported constructs, resource bounds and no-clobber behavior.
- **17 JavaScript tests passed**, frontend syntax checks and Clippy with warnings denied passed. Unsigned development app built. Existing `block 0.1.6` future compatibility notice remains.
- `scripts/smoke-mathjson.py` passed CLI import/export roundtrip, exact value `27021597764222980/3`, assumptions, unchanged input bytes, preserved existing destinations and unsupported `Log` rejection. Existing math-keyboard and reproduction smoke checks also passed after the LaTeX serializer change.

| Native macOS check (1280×800) | Result |
| --- | --- |
| Page identity / meaningful content | Passed: localized MathJSON disclosure, controls and native Save/Open dialogs. |
| Export | Passed: selected `a` saved as a new context JSON; original document stayed saved at revision 59 with the same save path. |
| Import / document isolation | Passed: matrix JSON opened as a second untitled document; original research document retained separately. |
| Notebook navigation | Passed: center switched from 3D to notebook automatically after import. |
| Computation / typesetting | Passed: two 2×2 matrices displayed with rows intact and product `[[7,10],[15,22]]` calculated in Rust. |
| Cleanup / original state | Passed: temporary imported document saved under `/private/tmp` and closed; original workspace restored saved at graph-edit-demo revision 59. |
| Visual health | Passed at tested size: no visible overlay, clipping or overlapping controls; matrix rows aligned. |
| Runtime console | Native automation does not expose console logs; syntax checks, IPC, dialogs and actual computation passed. |

Validation used `cua_repl` for native child WebViews and dialogs. The Browser skill is not installed, and static browser preview cannot exercise this workflow. Screenshot: `/private/tmp/symbio-mathjson-matrix-ja.jpg`.

The matrix product roundtrip exposed a pre-existing serializer bug: emitted LaTeX used a single row separator instead of the required double separator. The serializer now emits the correct delimiter, and roundtrip/determinant tests pass. Full MathJSON/OpenMath compatibility, non-ASCII identifiers, approximate literal provenance, calculus exchange, Windows/Linux and long-running large exchange inputs remain unimplemented or untested.

## Shared-coordinate 3D surface comparison (2026-11-01)

Target flow: 3D tab → layers popover → select another cell → inspect shared-axis surfaces/legend → add an unsupported cell → remove only its warning layer.

- **132 Rust tests passed** (77 core, 55 desktop). Eight new renderer/axis cases cover selection limits/duplicates/branch IDs, separated constant planes, one shared scale, small nonzero heights, individual failures, assumptions/holes/ranges, single-surface compatibility and generation/comparison frame stamps.
- **19 JavaScript tests passed**, including two axis-frame validity cases covering document changes, comparison membership, revision, camera/ranges, height extents and hidden/working states. Syntax checks and Clippy with warnings denied passed; unsigned development bundle built. Existing `block 0.1.6` future compatibility notice remains.
- Mesh data stays in Rust/GPU memory. View-state tests confirm geometry buffers are not serialized into WebView state.

| Native macOS check | Result |
| --- | --- |
| Identity/content | Passed: native Symbio 3D panel, localized layers popover, meaningful GPU geometry and numeric axis labels. |
| Two surfaces | Passed: `sin(x) cos(y)` and `(x²+y²)/4−1`, shared x/y `[-2,2]`, common z `[-1,1]`, blue/orange legend and `2/2 · 8450` valid samples. |
| Partial failure | Passed: adding `z=x+t` preserved both valid surfaces and showed a warning on the unsupported cell; footer showed `2/3`. |
| Legend removal | Passed: removing only the warning cell returned to `2/2`, without changing research revision 5 or save status. |
| Camera | Passed: keyboard rotation retained both surfaces and shared axes. |
| Save As context | Passed: new temporary copy retained research revision 5, reset comparison selection and returned to one 4225-sample surface. Original/copy project bytes were identical. |
| Final popover/keyboard | Passed: compact checkbox/name spacing in final build; Escape closed the popover. |
| Visual health | Passed at observed 1280×800 and enlarged 1430×1080: readable controls/legend, no visible overlay or overlapping formula/mesh viewport. |
| Original workspace | Restored saved graph-edit-demo revision 59 and its notebook view; mathematical content was not edited. |
| Runtime console | Native UI automation does not expose JS console logs. Syntax checks and actual IPC/GPU interactions passed. |

Native checks used `cua_repl`, since GPU surfaces and child WebViews cannot be validated by a static browser preview. The Browser skill is not installed. Screenshot evidence: `/private/tmp/symbio-surface-comparison-ja.jpg`.

Uniform 65×65 sampling, solid-surface occlusion and float64 uncertainty remain limitations. Adaptive sampling, scientific figure export, Windows/Linux, device loss, large/long sessions, and live formula/assumption edits while a comparison remains open were not separately exercised in this native pass. Their shared sampling semantics are covered by Rust cases. Comparison is a temporary current-branch viewport configuration, not stored research history.

## 2026-11-01 · Optional local CAS factorization

Flow under test: Structure & history → configure existing local Python/SymPy → open a temporary polynomial document → factor from the notebook → see typeset factors and bounded symbolic evidence → save the temporary document and return to the original.

- `cargo test --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop`: **137 passed** (82 core, 55 desktop). Added exact product validation, malformed/fabricated responses, input bounds, unsupported scope, worker timeout/termination and invalid transport tests.
- `cargo clippy --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop --all-targets -- -D warnings`, `cargo fmt --all --check`, JavaScript syntax checks and `git diff --check`: passed. Existing dependency future-compatibility notice for `block 0.1.6` remains.
- `node --test scripts/*.test.mjs`: **19 passed**.
- `python3 scripts/smoke-cas.py`: real isolated local Python/SymPy 1.14.0 transport passed for rational coefficients, repeated factors, irreducible quadratic, zero/constants, recorded recipes, reference-dependent recalculation, Undo/Redo, clearing failed outputs, unsupported multivariate/variable-denominator inputs, default skipped external replay, explicit successful replay and unavailable-runtime replay errors.
- `python3 scripts/smoke-reproduction.py`: existing Rust-only replay, checksum rejection, immutable source, no-clobber and changed expected-result detection passed.
- `python3 scripts/build-dev-app.py`: native development bundle built successfully.
- Browser plugin/skill is not available in this session. Native CUA was used because the target is the actual Tauri child-WebView application, including application-owned preferences and native file dialogs. GUI confirmed disabled factor action before configuration, a successful `SymPy 1.14.0` probe, and `x^4 - 1 → (x − 1) · (x + 1) · (x² + 1)` with **Symbolic** status and Rust coefficient-check condition. Screenshot inspection confirmed the three-column layout and readable MathML result. Only a temporary document was modified/saved; the original returned to saved revision 59.

Limits: this is not a general CAS conformance benchmark, formal proof, independent irreducibility check, or OS sandbox. General equation/integration adapters remain unfinished. External replay was exercised through the real CLI; native package-picker replay of this operation and other operating systems have not been exercised in this environment.

## 2026-11-01 · MathJSON calculus exchange

Flow under test: Structure & history → MathJSON import → select the calculus example → inspect the authored definite integral and exact result → save the test document and return to the original.

- `cargo test --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop`: **143 passed** (87 core, 56 desktop). MathJSON now has 14 tests, including calculus decoding/encoding, exact results, authored binders rather than cached outputs, nested scope, free variables in limits, context preservation, object forms, invalid arities/shapes/variables, bounded evaluation and scope-safe LaTeX operands. Dock tests cover the selected tab in docked and floating groups.
- `python3 scripts/smoke-mathjson.py`: CLI import/export/reimport passed for derivatives, integrals, finite sums/products, exact large numbers, explicit assumptions, immutable inputs and no-clobber destinations.
- `cargo clippy --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop --all-targets -- -D warnings`, `cargo fmt --all --check`, JavaScript syntax check, and `git diff --check`: passed. The existing `block 0.1.6` future-compatibility notice remains.
- `node --test scripts/*.test.mjs`: **19 passed**. Native development bundle rebuilt successfully.
- Browser plugin/skill is not available. Native CUA tested the actual Tauri child-WebViews and native import/save dialogs. `examples/calculus.mathjson.json` imported into a separate untitled document and displayed `∫₀² x² dx` and exact `8/3` in MathML. Screenshot inspection confirmed readable notation and the three-column layout. GUI QA exposed untranslated calculus kind labels and a redundant notebook-selection warning after import; both were fixed and the same native import flow was rerun successfully with “積分” and ready status. English/Japanese/Simplified Chinese labels and tooltips were updated.
- The original research document was not edited and returned to its saved revision 62. Test documents alone were saved; one initial test filename was saved under application support, and the final test was saved under `/private/tmp/symbio-mathjson-calculus-gui-20261101.symbio`.

This extends exchange support, not general integration/differentiation algorithms. Approximate literals, implicit binders, infinite limits and functional `Derivative` remain rejected. Native export of the new binders was not separately exercised; CLI export/reimport and core structure roundtrips passed. Windows/Linux native validation remains pending.

## 2026-11-02 · Exporter kernel environment manifest

Flow under test: Structure & history → Reproduction → open a test package → inspect result replay and separate kernel-environment status → expand the exporter/verifier comparison table.

- `cargo test --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop`: **147 passed** (91 core, 56 desktop). Added manifest capture/roundtrip, environment-only differences with matching mathematical replay, legacy checksum compatibility, unknown/oversized/malformed metadata, checksum tampering and unsupported manifest-version tests.
- `python3 scripts/smoke-reproduction.py`: real CLI verified matching exporter fields, a resealed synthetic OS difference without result mismatch, legacy packages without manifests, immutable source/no-clobber writes and existing checksum/result-mismatch behavior.
- `cargo clippy --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop --all-targets -- -D warnings`, formatting, JavaScript syntax and diff whitespace checks passed. Existing `block 0.1.6` future-compatibility notice remains.
- `node --test scripts/*.test.mjs`: **19 passed**. Development app bundle rebuilt.
- Browser plugin/skill is not available; native CUA exercised the actual Tauri child-WebViews and package picker. A temporary package with a synthetic `different-os` exporter value showed **replay matched** for two cells, **kernel fields differ**, and the comparison table's highlighted `different-os → macos` row. Real local OS/CPU targets and lockfile digests were shown as text. The synthetic difference tests reporting, not cross-platform execution. English, Japanese and Simplified Chinese labels/tooltips were added. Environment details reuse the existing disclosure state preservation.
- The read-only verification initially left the research document saved at revision 62 with the same selected surface cell. Subsequent external UI interaction produced new unsaved changes at revision 66 and selected the 3D tab; these changes were left untouched, and no further restart or save was attempted. Verification itself did not add a session document, change its save destination or execute external CAS. Test packages were created under `/private/tmp/symbio-environment-20261102`.
- The final small disclosure-state persistence change passed syntax/build checks; a native restart to recheck that detail was deferred to avoid disturbing the newly unsaved edits. The table and separate statuses were already verified in the preceding native flow.

The manifest describes the exporting build's bounded fields. It is not a full original-calculation environment, compiler/source/binary fingerprint, environment restore facility, digital signature or mathematical proof. Windows/Linux native validation and full environment capture remain pending.

## 2026-11-02 · Stateless local MCP tools

Flow under test: launch the real CLI subprocess in MCP mode → initialize and notify ready → discover schemas → request independent calculations/previews → inspect structured evidence → reject document/CAS authority → terminate cleanly on EOF or an oversized frame.

- `cargo test --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop`: **151 passed** (4 MCP CLI, 91 core, 56 desktop). New tests cover lifecycle gating, notifications, invalid JSON-RPC IDs/methods/cursors, exact calculus, assumption isolation, approximation trust, SEG inspection, preview, error codes, forbidden fields/CAS operations and framing limits.
- `python3 scripts/smoke-mcp.py`: independent real stdio client passed. Python JSON Schema validation checked both advertised schemas and structured success/error results; serialized text and structured content matched. Exact integral, assumption-sensitive simplification, fresh-call isolation, numeric evidence, matrix LaTeX preview, parse/domain/reference errors, rejected authority and clean EOF were exercised. An inherited fake CAS executable was not run, and the scratch working directory was not changed. Oversized input terminated with stderr and no protocol pollution.
- `CARGO_NET_OFFLINE=true python3 scripts/smoke-cli.py`: previous project-based JSONL CLI behavior, persistence, actor attribution, conflict/parse rollback and LaTeX handling passed after adding the separate MCP entry point.
- `cargo clippy --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop --all-targets -- -D warnings`, formatting and diff checks passed. Existing dependency future-compatibility notice for `block 0.1.6` remains.

No GUI was changed or restarted; ongoing user edits were left alone. This is a local synchronous stdio subset, not an HTTP server, project mutation channel, natural-language planner, full MCP ecosystem certification or general CAS gateway. Third-party MCP host integration, cancellation/progress and permission-bound notebook proposals remain pending. No client configuration or packages were automatically installed. The Python jsonschema package is only a locally available validation dependency, not a runtime requirement.

## 2026-11-02 · Exact high-degree real polynomial solving

Flow under test: explicit LaTeX equation → configured local factor worker → independent exact coefficient-product check → Rust linear/quadratic solving of every factor → complete real solution set or an unsupported error → dependency replay and saved reproduction.

- `cargo test --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop`: **156 passed** (96 core, 4 MCP CLI, 56 desktop). New core cases cover exact radical roots, proportional-factor normalization, root deduplication, empty real sets, nonzero constants, identity rejection, sign filtering and unsupported irrational sign filtering, zero roots, incomplete/incorrect factor rejection and the bounded 32-entry persisted solution capacity.
- `python3 scripts/smoke-cas-solve.py`: actual local SymPy 1.14.0 passed for quartic/sextic equations, repeated factors, four exact irrational roots, zero roots, and 32 distinct rational roots at the degree limit. Residual cubic/quartic factors, variable denominators, identities and extra symbols were rejected without publishing partial roots. CLI transactions rolled back failed operations; reference/source and assumption edits, Undo/Redo, stale-output clearing, save/cache reload, default skipped external replay, explicit successful replay and unavailable-runtime errors passed.
- `python3 scripts/smoke-cas.py`: previous factorization behavior and recipe/reproduction regression passed after sharing its bounded worker protocol with solving.
- `python3 scripts/smoke-mcp.py`: stateless MCP lifecycle, schema/evidence and authority isolation regression passed. The new external operation is not exposed through MCP.
- `cargo clippy --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop --all-targets -- -D warnings`, formatting, JavaScript syntax and diff checks passed. `node --test scripts/*.test.mjs`: **19 passed**. The existing `block 0.1.6` future-compatibility notice remains. Native development bundle rebuilt; engine identifier is now `symbio-rust/0.7.0`.
- GUI source adds a separate equation action `x = ? · CAS`, disabled until a local interpreter is configured, and updates the glyph when the chosen variable changes. Operation/evidence labels and CAS scope tooltips support English, Japanese and Simplified Chinese.
- Native CUA inspected the running application before attempting GUI QA and found **unsaved revision 66**. No restart, save, document switch or calculation was performed in that application. The new native button/visual flow is **pending**, to preserve ongoing work; backend, real CAS, CLI, persistence and replay are verified. No new rendered-UI success is claimed.

This is not unrestricted polynomial root isolation, arbitrary equation solving or a formal proof. Every rational factor must have degree one or two; higher-degree residual factors reject the whole operation. Root multiplicities are not returned. The existing offline quadratic solver and stateless MCP authority remain unchanged. Complete root-list formatting and native controls still require the pending GUI pass.

## 2026-11-04 · Bounded OpenMath XML exchange

- `cargo test --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop`: **162 passed** (102 core, 4 CLI, 56 desktop). Six new XML integration tests cover exact large integers/rationals, elementary functions, names/assumptions, reference expansion, equation solving, matrix construction, undefined domains, namespace resolution and rejected unsupported semantics. DTD/entity, foreign dictionary/namespace, external reference, unknown attribution, resource limits and no-clobber cases are included.
- `cargo clippy --locked --offline -p symbio-core -p symbio-cli -p symbio-desktop --all-targets -- -D warnings`: passed; the later notebook-reveal change also passed desktop Clippy and rebuilt the native app. Existing upstream `block 0.1.6` future-compatibility notice remains.
- `python3 scripts/smoke-openmath.py`: real CLI XML roundtrip passed, including `9007199254740993 + 1/3 = 27021597764222980/3`, real-domain context preservation for `sqrt(x^2)`, immutable sources/destinations and rejected DTD. Python ElementTree independently parsed exported XML; this is not Relax NG conformance or third-party mathematical interoperability validation.
- `python3 scripts/smoke-mathjson.py`: existing exact/calculus CLI exchange regression passed. All **19 JavaScript tests** passed; syntax, formatting and whitespace checks passed.
- Native macOS CUA verified XML import through the file picker while 3D was active, a separate document titled OpenMath, automatic notebook selection, and correctly typeset exact input/output fractions. This exposed a transient `panel_busy` reveal race; the bounded retry fix was rebuilt and the same 3D-to-notebook import then passed.
- Native XML export wrote `/private/tmp/symbio-openmath-gui-20261104.xml`; ElementTree confirmed OMOBJ, exact authored operands and explicit semantic context. The imported document was saved separately to `/private/tmp/symbio-openmath-gui-20261104-final.symbio` and closed.
- Native legacy MathJSON import and the previously pending high-degree CAS button were also verified: `examples/cas-polynomial.mathjson.json` produced a separate document, `x^6 = 1` returned `x ∈ {−1, 1}`, and the UI displayed Symbolic status, real-domain conditions and Rust factor-product checking. Only the test document was saved to `/private/tmp/symbio-cas-gui-20261104.symbio`.
- The original Polynomial study remained saved at **revision 66**, branch `graph-edit-demo`, selected `surface1`; its 3D tab was restored. No original source, assumptions or calculation was edited during this verification.

Limits: OpenMath calculus, matrix operations, approximate numbers and additional dictionaries remain unfinished. The context CD is experimental and local. Full schema validation and exchange against independent OpenMath applications remain unverified; no formal proof or full OpenMath implementation is claimed. Windows/Linux native dialogs and GPU backends remain pending.

## 2026-11-04 · Explicit OpenMath calculus

Flow: native OpenMath file picker → new integral document → typeset authored integral and exact result → XML export → independent XML inspection and fresh-process CLI import.

- Rust tests: **166 passed** (106 core, 4 CLI, 56 desktop). Four added integration tests exercise authored derivative/primitive/definite-integral/sum/product exchange; reversed and empty ranges; nested and shadowed binders; free parameters in limits and surrounding expressions; explicit assumptions and arbitrary-constant conditions; unsupported and singular integrands; malformed/foreign binders, unsupported interval meanings and function application at a different point. Existing scalar, matrix and XML-boundary regressions passed.
- Clippy with `--all-targets -- -D warnings`, formatting and JS syntax checks passed. All **19 JavaScript tests** passed. `smoke-openmath.py` verified authored calculus XML and exact `8/3` through separate CLI processes; `smoke-mathjson.py` passed unchanged. The unsigned development app rebuilt successfully; the existing upstream `block 0.1.6` future-compatibility notice remains.
- Native macOS CUA imported `examples/calculus.openmath.xml` from the original 3D view. The new document selected Notebook, displayed the integral from 0 to 2 of x² with its differential, and returned the fraction 8/3 marked exact/Symbolic. Screenshot inspection at 1280×800 found no clipping or overlay. The expanded OpenMath tooltip reflected the updated scope in Japanese; English and Simplified Chinese strings were updated in source.
- GUI exported `/private/tmp/symbio-calculus-openmath-gui-20261104.xml`. Python ElementTree found `calculus1.defint` and the explicit lambda variable; reimporting this GUI-produced XML with the real CLI returned `8/3`. Only the test document was saved separately to `/private/tmp/symbio-calculus-openmath-gui-20261104.symbio` and closed. The original saved Polynomial study stayed at revision 66, branch `graph-edit-demo`, selected surface1; the 3D tab was restored.

The Browser plugin/skill was unavailable. Native CUA was used because the target includes actual Tauri child WebViews and native file dialogs, which the read-only browser preview cannot exercise. Native console collection was not available; no console-clean claim is made. GUI derivative/sum/product workflows, other locales, other operating systems and independent OpenMath applications were not exercised in this pass. Full XML schema conformance remains unverified. General function evaluation, partial derivatives and additional interval forms remain unsupported; this change connects existing Rust calculus and does not add a general CAS integration algorithm.

## 2026-11-04 · Matrix determinant and transpose exchange

Flow: native OpenMath picker → rectangular transpose as a new document → typeset authored matrix/operator and exact result → MathJSON export → fresh-process CLI import.

- Rust: **170 passed** (110 core, 4 CLI, 56 desktop). Four additional tests cover determinant/transpose roundtrips and nesting, rectangular orientation, exact rational determinant `11/72`, singular determinant zero, authored LaTeX/reference expansion without owner mutation, and rejected scalar/nonsquare/foreign/extra-axis operations. The transpose result is independently checked as a 3×2 matrix with row-major entries `1,4,2,5,3,6`.
- Clippy `--all-targets -- -D warnings`, formatting, JavaScript syntax and whitespace checks passed. All **19 JavaScript tests** passed. Real CLI OpenMath smoke additionally crossed XML → project → authored MathJSON `Transpose` → project → XML, retaining its matrix operator and immutable source. Existing MathJSON/calculus smoke passed. The unsigned native app rebuilt successfully; the upstream `block 0.1.6` future-compatibility notice remains.
- Native macOS CUA imported `examples/transpose.openmath.xml` from the original 3D workspace. Notebook selected automatically and displayed the 2×3 authored matrix with superscript T above the exact 3×2 result. Screenshot inspection at 1280×800 found no clipping or error overlay. Updated Japanese MathJSON/OpenMath tooltips were visible; English and Simplified Chinese strings were updated in source.
- GUI exported `/private/tmp/symbio-matrix-exchange-gui-20261104.json`; direct JSON inspection confirmed the authored `Transpose` head instead of cached output. A fresh CLI import returned `matrix(3x2; 1, 4, 2, 5, 3, 6)`. Only the test document was saved to `/private/tmp/symbio-matrix-exchange-gui-20261104.symbio` and closed. The original saved Polynomial study remained revision 66 on `graph-edit-demo`, selected surface1, and its 3D tab was restored.

Native CUA was used for actual Tauri child WebViews and native file dialogs; the Browser plugin/skill was unavailable and the read-only web preview cannot exercise this path. Native console collection was unavailable, so no console-clean claim is made. Native determinant import, other UI locales, Windows/Linux, full schema conformance and independent OpenMath applications were not exercised in this pass. Other OpenMath matrix arithmetic, vectors, conjugate transpose and tensor axis options remain unsupported. This connects the existing exact matrix evaluator; it adds no formal proof or arbitrary-size linear algebra.

## 2026-11-04 · Core source fingerprint

Reproduction tests: **18 passed**, including optional legacy fingerprints, changed source identity reported separately from successful mathematical replay, and malformed digests rejected. Core/desktop all-target Clippy with warnings denied passed, as did JavaScript syntax and whitespace checks. The source fingerprint is captured during compilation, never by reading source files during package verification. Native environment-table rendering remains pending to protect ongoing unsaved research edits; no restart or document modification was performed. Full application/binary provenance remains unfinished.

## 2026-11-04 · Selected core build configuration

Reproduction tests: **19 passed**. New coverage verifies optional legacy configuration, changed debug-assertion mode reported separately from successful mathematical replay, and rejected malformed targets, optimization values and feature strings. Core/desktop all-target Clippy with warnings denied, JavaScript syntax, all **19 JavaScript tests** and whitespace checks passed. The build script captures target/optimization/features and the core records effective debug assertions. The GUI comparison table now includes this object with English/Japanese/Simplified Chinese row labels. Native rendering remains pending; no application restart or document edit was performed. This records selected core conditions, not full compiler/linker flags, dependency profiles, native library provenance or a final binary fingerprint.

## 2026-11-04 · Explicit MathJSON matrix products

MathJSON/OpenMath integration tests: **29 passed** (17 MathJSON, 12 OpenMath). The new matrix-product test checks exact entries 19,22,43,50, noncommutative operand order, authored operator roundtrip and rejection of wrong shapes, scalar operands, extra arguments and Hadamard products. Core all-target Clippy with warnings denied passed. A real CLI import/export independently confirmed the exact result and exported MatrixMultiply head. No native app restart or document edit was performed. Existing Multiply matrix inputs retain their meaning; binary MatrixMultiply is the explicit export for two matrix operands. OpenMath matrix products, vectors and element-wise products remain unsupported.

## 2026-11-06 · MathJSON inverse matrices

MathJSON/OpenMath integration tests: **30 passed** (18 MathJSON, 12 OpenMath). New coverage verifies exact inverse entries −2,1,3/2,−1/2, mathematical roundtrip, preserved singular-matrix domain errors and rejected scalar/nonsquare/extra-argument inputs. Core all-target Clippy with warnings denied, JavaScript syntax and whitespace checks passed. Real CLI import/export/reimport retained the exact inverse result. Existing Rust matrix algorithms are reused; generated LaTeX can normalize Inverse to a matrix power of −1. Native UI was not restarted or edited, and no new rendered-flow verification is claimed. OpenMath inverse exchange remains unsupported.

## 2026-11-06 · Bounded curvature-based surface refinement

Desktop tests: **57 passed**. The new test checks that a plane retains 4225 samples while sin(40x)cos(30y) refines to 16641 samples with finite mesh coordinates. Existing domain masks, assumptions, independent ranges, shared-height comparison and small-height regression tests passed. Refinement is limited to one global level and only fully valid initial grids; it is a display heuristic without certified error or feature-detection guarantees. Native GPU appearance/performance on real research models remains unverified in this pass; no running document or app was modified.

## 2026-11-06 · Surface midpoint probes

Desktop tests: **58 passed**. A lattice-aligned cosine plus a linear slope refines from 65×65 to 129×129 and recovers the negative trough; a constant remains at base resolution. Existing range, domain-mask and comparison regressions pass. Cell-center interpolation discrepancies and invalid center values trigger at most one global refinement; up to 4096 extra evaluations remain entirely in Rust. Native GPU rendering/performance remains unverified. This does not guarantee narrow-feature or singularity detection; the existing steep-triangle rejection still applies after refinement. No running document was edited or restarted.

## 2026-11-06 · Interior surface-domain masking

Desktop tests: **59 passed**. A new regression constructs a square-root surface with all 129×129 lattice points valid but undefined centers in the first cell column; emitted triangles must not bridge that interior hole. Existing masking, adaptive sampling, ranges and comparisons passed. Final mesh generation skips both triangles and wire lines for detected invalid centers, including at the final refinement level. Core samples/omissions still count lattice points only. Desktop all-target Clippy with warnings denied passed after adopting array-chunk iteration in the test. Native GPU rendering and performance remain unverified; the new checks add at most 16384 numeric evaluations per final surface. Probe-based masking is not an exhaustive domain or singularity analysis. No running document was changed or restarted.

## 2026-11-06 · Embedded CAS worker provenance

Full Rust regression suite: **178 passed** across core, CLI and desktop. `scripts/smoke-source-fingerprint.py` runs the actual compiled build script against an isolated temporary source tree: repeated inputs produce the same digest, worker-only Python edits change it, and worker deletion changes it again. Core source manifest v2 now includes .rs and .py files under src, retaining sorted relative names and length framing. Previous v1 digests intentionally differ without invalidating mathematical replay. This does not fingerprint installed Python/SymPy or the final binary. No running application or research document was modified; the GUI provenance table remains unverified.

## 2026-11-06 · Rational-affine elementary integration

Core regression suite: **117 passed**; core all-target Clippy with warnings denied and whitespace checks passed. New tests differentiate the computed primitives of sin(2x+1), cos(−3x+2), exp(x/3−1/2) and sin(x), comparing their values with the original integrands at four real points to tolerance 1e−12. Nonlinear and unknown-symbol slopes are rejected. Existing structured, MathJSON/OpenMath, domain and replay regressions passed. Real CLI import of an explicit integral of exp(2x) returned exp(2x)/2 with exact precision and the arbitrary-constant condition. Numeric spot checks are regression evidence, not a formal proof. Native UI was not restarted or modified; general integration, branch-sensitive logarithmic primitives and symbolic slopes remain unfinished.

## 2026-11-06 · Rational scaling of elementary integrals

Core suite: **117 passed**; core all-target Clippy with warnings denied passed. Primitive differentiation tests now include 3 sin(2x+1), cos(3x)/5 and −2 exp(2x)/3, with rational coefficients normalized through the normal simplification path before integration. Rejection tests include x sin(x), sin(x)/x and division by zero. Real CLI import of an explicit integral of 3 exp(2x) returned an exact symbolic primitive. These regression checks do not constitute formal proof. No native app or document was modified. Symbolic constant factors, integration by parts and general variable-dependent denominators remain outside this extension.
