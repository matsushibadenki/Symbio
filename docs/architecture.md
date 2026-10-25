# Architecture / 構成 / 架构

```text
Tauri child WebViews              CLI (JSONL)
  commands + bounded snapshots      commands
              │                       │
              └──────────┬────────────┘
                    symbio-core
                 AppCore / Project
         revision check → clone → validate → commit
          ┌──────────────┼──────────────┐
      SEG / parser    algebra       branch / audit
      assumptions    rational       undo / redo
      dimensions     float64        ProjectStore
          │                              │
    Rust CPU samples                atomic JSON file
          │
    ThreeX retained Scene → wgpu Surface → native plot window
```

## Rust ownership

`crates/symbio-core` is GUI-independent. `Project` owns the hash-interned node arena (append-only between collections), branch cells, assumptions, selection and history. `AppCore` owns revision checks and bounded undo/redo. There is one `Shared<Session>` in Tauri app state. Child WebViews hold only presentation snapshots and uncommitted editor drafts, never an authoritative document or evaluator. The draft registry prevents losing pending edits on branch/history operations.

`apps/desktop/src/layout.rs` owns a binary split/tab tree. `workspace.rs` manages stable child WebView identities, native reparenting, drop previews and revision-checked transitions. `window_placement.rs` restores geometry against current monitor bounds and DPI. The layout is separate from the mathematical document.

`Plot` is app-owned Rust state holding the retained scene, GPU device/surface and scale. JavaScript sends only a cell ID. CPU sampling runs on a blocking worker; CAMetalLayer creation and presentation run on the macOS UI thread. Scene geometry and pixel frames never cross the JavaScript bridge. This initial plot is a separate native window, not yet a dockable GPU panel.

## Semantic representation

Nodes contain rational numbers, symbols, constants, arithmetic/functions, integrals, derivatives, finite aggregates, matrices, equations and finite solution sets. Node IDs refer only to earlier nodes. Structural interning does not commute operands or claim mathematical equality. Parsing source and validated graph roots must agree when loading a file.

Expression-mode definitions use bounded ASCII identifiers and explicit multiplication. LaTeX mode uses a bounded Rust parser with implicit multiplication; both parsers intern the same node types. Each cell persists its source format alongside its source and root. Live previews use a temporary Rust graph, return only generated MathML/canonical text/free symbols/shape kind, and never modify the document. The UI suppresses stale preview responses and hides old output while the source is a draft. Scalar domains are unspecified, real, nonnegative, positive and complex. Optional integer dimensions represent powers of mass/length/time. Unknown dimensions propagate as unknown; incompatible known additive dimensions fail. Fractional dimensions, units/conversions, complete symbolic domain reasoning are not yet modeled. Matrix shapes are checked through scalar/matrix/relation/solution types before commit. Integral and aggregate dummy variables have scoped free-symbol analysis and capture-avoiding substitution.

The kernel conservatively declines unsupported transforms. Symbolic output is not a proof certificate. `actor: ai` is caller attribution, not authenticated identity. CLI callers have local file access already; there is no external server or trust elevation.

## Persistence and concurrency

Every mutation supplies the current revision. Validation precedes commit; conflicts and domain errors leave the document unchanged. Assumption changes recalculate affected current-branch cells using persisted calculation recipes. Branches share immutable graph nodes but own distinct cells and assumptions. Reference nodes resolve authored definitions within the active branch; branch merging remains unsupported.

An advisory file lock excludes cooperating writers. Saving compares file bytes with the loaded baseline, then writes/syncs a same-directory temporary file, atomically renames it and syncs the directory on Unix. Files larger than 8 MiB are rejected. Invalid/version-mismatched documents are not silently replaced. This detects external changes but is not a distributed filesystem transaction.

Resource limits include 16,000 SEG nodes, 128 cells per branch, 16 branches, 64 assumptions, 2,048-byte source, parser depth 64 and 512 tokens, degree 32, 32 undo states, 256 audit events and 64 KiB CLI request lines. Automatic collection bounds accumulated unused structure; the graph limit still applies to live data and to each attempted computation.

## Extension boundaries

New CAS, proof and simulation adapters should return explicit engine/version, assumptions, precision, trust and evidence, through the same transaction layer. Mature CAS/FLINT/Lean/Julia integration is intentionally deferred. An e-graph must carry conditional rewrite validity and bounded time/memory. GPU numeric acceleration must not inherit certified status from a CPU result automatically.

A future MCP gateway should invoke this core with explicit file/network grants; it should not automate the GUI. Content-addressed experiment blobs, complete audit replay and incremental dependencies belong in separate storage/runtime crates as they become substantial. The initial cohesive core avoids empty crates that imply nonexistent engines.

## Formula presentation and structured evaluation

Rust generates MathML with native fraction, integral limit, derivative, matrix-table and equation elements. WebViews handle typography, horizontal scrolling and a resizable focus view. No TeX macros or external renderer execute. The computation keyboard uses supported LaTeX templates; the supplied source palette remains available separately as reference notation in notes. Placeholder navigation is local editor state.

`structured.rs` evaluates bounded exact matrix algorithms, finite sums/products, supported primitives and quadratic real solutions over the same graph. Indefinite primitives retain an arbitrary-constant condition and cannot silently become numeric results. Numerical structured results carry approximate trust and decimal MathML presentation. Input/output shapes are validated again when loading saved documents.

## KOMYAKU source bridge

`exchange.rs` exposes bounded, GUI-independent computation and single-equation `.symbio` export/import. The KOMYAKU desktop uses this crate as a sibling path dependency. Each request builds an isolated temporary Project with the caller's explicit assumptions, avoiding sample cells/default assumptions or other document content. Computed outputs remain derived data in KOMYAKU. Its Canonical source edits retain stable IDs and the existing Yjs/checkpoint pipeline. The adapter does not perform filesystem or network I/O.

## Dependency runtime

`dependencies.rs` validates a branch-local cell DAG, resolves explicit `reference` nodes and expands authored definitions before algebra. It computes a deterministic topological order, affected downstream closure and conservative assumption users. The transaction layer replays each affected cell's persisted calculation recipe. Mathematical failures clear stale outputs and retain editable source/error state; graph structure and resource failures roll back atomically.

Cached resolved roots are checked against authored definitions during file validation. Loading initializes missing legacy caches and recovers audited calculation parameters when available. Derived recipes/results are branch-local and part of Undo snapshots. The WebView draws at most 128 cell nodes from Rust-owned edges; it does not resolve references or evaluate formulas. Diagram geometry uses CSSOM properties compatible with the existing strict CSP, and skipped-level edges route around intermediate nodes.

The inspector opens the dependency diagram in a dedicated native window with adjustable zoom. All graph windows subscribe to the same Rust document; node selection and recalculation use the common command API.

## Native 3D surface panel

`surface3d.rs` owns cell selection, independent x/y domains, orbit camera and bounded CPU sampling over resolved SEG definitions. Edit publication schedules a new generation; stale sampler jobs cannot replace newer selections or ranges. Unsupported / undefined inputs clear the retained mesh and publish an explicit view error. These are session view settings, separate from the mathematical document and its Undo stack.

`surface_gpu.rs` owns a shared device, triangle / line buffers and native presentation targets for every possible dock host. Host surfaces are created on AppKit's thread before child WebViews. Each render uses Rust dock geometry for viewport / scissor and shares mesh buffers when the panel floats or returns. A transparent WebView carries opaque controls, MathML and pointer / keyboard commands above the native surface. No texture readback, frame transport or JavaScript geometry generation occurs. ThreeX supplies camera projection mathematics; native region and retained buffer patterns follow the existing ThreeX/ATOM implementation.

Dock layout version 4 migrates v1–v3 layouts to include the new surface panel, preserving existing panel identities and placement. The new panel joins a docked Notebook as a tab, or floats when Notebook is already floating. New and reset layouts use three columns with a wide central Notebook / 3D tab group. Existing v4 custom layouts remain saved until Reset workspace is requested. Floating-window preferences migrate three host slots to four.

`axes3d.rs` builds colored boundary rulers with bounded 1/2/2.5/5/10 tick spacing. Mesh normalization and ruler mapping use the same independent domain bounds and original sampled z extrema. Rust projects small semantic text anchors with the exact GPU camera and WebGPU screen convention. The transparent WebView lays out those labels and suppresses overlaps; no vertex or frame data crosses IPC. GPU ruler line storage is a bounded reusable buffer. The controls reserve 210 logical pixels above the viewport; native scissor and WebView stage share that geometry. Domain updates validate both axes before publication, and invalidate outstanding sampler generations while holding the same view-state lock.

## Collection and graph-based editing

`collection.rs` marks cell input, resolved-definition and output roots across all branches, then rebuilds live nodes in topological order with structural interning. It remaps a cloned document and validates it before replacing any state. Cell IDs and semantic metadata stay stable; SEG IDs are revision-scoped. `AppCore` compacts after a successful mutation at 2,048 total / 256 unused nodes and retries a resource-limited command once against a compacted clone of the original state. Failed edits never publish partial collection. Preview compaction remains isolated. History commands refer to cell IDs and authored sources; Undo snapshots retain separate complete graphs.

Graph editors use the same Define / DefineLatex commands as Notebook and the same pure contextual preview. WebViews retain only local drafts, including a base definition stamp to detect edits from another panel. Apply preserves the existing calculation recipe and triggers the dependency closure. The shared draft registry protects branch/history/collection commands and native graph-window closing; Destroyed events release its draft entry. Graph-window Save uses the existing shared-document save protocol. Graph editing currently edits authored cell source rather than directly manipulating SEG subexpressions or edges.

## Desktop document locations

The Rust `Session` owns the active `AppCore`, `ProjectStore` lock and a monotonically increasing document generation. Desktop snapshots flatten the kernel snapshot and add `generation` and `path`. Opening another file or changing its location increments this generation; revisions remain local to the mathematical document. The WebViews clear document-specific drafts and inspection caches on replacement and ignore older generations. Editing, saving, draft registration, previews and inspection require the expected generation. Native file operations are serialized and run off the UI thread; all panels become inert while a picker is open. Candidate documents are validated before the active session is replaced. Save As changes the active store only after its atomic write succeeds. Location preferences are best-effort and never turn a completed document write into a reported failure.

Blank documents use `Project::empty` and a session with no `ProjectStore`. Their desktop snapshot path is null. Save As acquires the first file lock and publishes the location only after a successful write. The shared switch confirmation handles both saved documents and untitled documents; cancelled location selection preserves the active document. No sample graph, assumptions, audit history or Undo state is inherited.
