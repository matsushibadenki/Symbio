# Reuse and provenance

The user authorized reference/reuse of the local ThreeX and ATOM directories. Their source directories were left unchanged. This project has no runtime dependency on sibling-directory paths: the required ThreeX sources are vendored here.

| Symbio path | Source | Adaptation |
|---|---|---|
| `vendor/threex/crates/core` | `ATOM/vendor/threex/crates/core` (ThreeX-derived retained scene implementation) | Copied unchanged |
| `vendor/threex/crates/renderer` | `ATOM/vendor/threex/crates/renderer` (ThreeX wgpu renderer with ATOM native integration) | Copied unchanged |
| `apps/desktop/src/layout.rs`, `layout/tests.rs` | `ATOM/apps/desktop/src/layout.rs`, `layout/tests.rs` | Object/notebook/inspector identities, default layout, Symbio model |
| `apps/desktop/src/workspace.rs` | `ATOM/apps/desktop/src/workspace.rs` | Symbio events, panel titles, full-height notebook, removed ATOM viewport behavior |
| `apps/desktop/src/window_placement.rs` | `ATOM/apps/desktop/src/window_placement.rs` | Symbio names/events; native monitor/DPI recovery retained |
| `apps/desktop/ui/dock.js` | ATOM native WebView docking approach | Small ES-module controller over Rust commands; nested splitter traversal/focus |
| `crates/symbio-core/src/storage.rs` | `ATOM/crates/atom-core/src/storage.rs` patterns | Symbio JSON format, validation, lock, baseline comparison, atomic save |
| `scripts/build-dev-app.py` | `ATOM/scripts/build-dev-app.py` | Symbio executable / identifier / minimal development bundle |
| `apps/desktop/icons/icon.png` | ATOM development icon | Build resource only; not a new product identity |

The original `/Users/Shared/Program/Tauri/ThreeX` implementation and ATOM's integrated ThreeX copy were inspected. The integrated copy supplies the native surface API needed by this app. Mathematical graph/parser/algebra/runtime, CLI, notebook UI and GPU sampling adapter are new Symbio code.

[Vendor manifest](vendor-manifest.json) records SHA-256 for every copied ThreeX file and its absolute local source. It describes the actual local bytes, including any preexisting upstream working-tree changes, rather than asserting an upstream release or commit.

No redistribution license was supplied for the local reused application sources. This deliverable does not assign a new license to those files or claim that the entire app can already be publicly redistributed as open source. Confirm the source owners' licensing before publishing a repository or binary. Cargo dependency licenses remain their respective upstream licenses.

The visual concept was generated for this task and implemented in native HTML/CSS/MathML. Runtime UI does not ship the concept image. Screenshots in `docs/screenshots` are captures of the actual development app, not mockups.

## Math keyboard

The user additionally provided `/Users/littlebuddha/Desktop/mathkeyboard.html`. Its HTML declares the canonical `https://xs927991.xsrv.jp/math-keyboard/` and references `script.js`; the companion script was fetched read-only from that public origin on 2026-10-23. Original bytes and SHA-256 hashes are archived in `vendor/mathkeyboard/` and `vendor/mathkeyboard/provenance.json`.

Only the static `keys` array is extracted into `apps/desktop/ui/math-keyboard-data.js`. The original script, remote MathJax, discovery metadata and HTML are **not executed or embedded** in Symbio. No dependency on the website is introduced. Original fraction/root/power/bracket glyphs and LaTeX snippets are reused; equivalent CAS-compatible templates and digit/function keys are added for expression input. Original `noop` category/modifier buttons are not exposed as nonfunctional controls.

`math-keyboard-edit.js` implements selection-aware insertion and bounded draft undo; `math-keyboard.js` renders compact categories. Source inputs feed the existing Rust transaction flow. The full notation palette targets research-note text, so unsupported integrals/limits/set notation never masquerade as executable expressions. No LaTeX parser or renderer is claimed by this addition.
