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

`crates/symbio-core` is GUI-independent. `Project` owns the append-only, hash-interned node arena, branch cells, assumptions, selection and history. `AppCore` owns revision checks and bounded undo/redo. There is one `Shared<Session>` in Tauri app state. Child WebViews hold only presentation snapshots and uncommitted editor drafts, never an authoritative document or evaluator. The draft registry prevents losing pending edits on branch/history operations.

`apps/desktop/src/layout.rs` owns a binary split/tab tree. `workspace.rs` manages stable child WebView identities, native reparenting, drop previews and revision-checked transitions. `window_placement.rs` restores geometry against current monitor bounds and DPI. The layout is separate from the mathematical document.

`Plot` is app-owned Rust state holding the retained scene, GPU device/surface and scale. JavaScript sends only a cell ID. CPU sampling runs on a blocking worker; CAMetalLayer creation and presentation run on the macOS UI thread. Scene geometry and pixel frames never cross the JavaScript bridge. This initial plot is a separate native window, not yet a dockable GPU panel.

## Semantic representation

Nodes contain a rational number, symbol, binary arithmetic operator or supported single-argument function. Node IDs refer only to earlier nodes. Structural interning does not commute operands or claim mathematical equality. Parsing source and validated graph roots must agree when loading a file.

Expression-mode definitions use bounded ASCII identifiers and explicit multiplication. LaTeX mode uses a bounded Rust parser with implicit multiplication; both parsers intern the same node types. Each cell persists its source format alongside its source and root. Live previews use a temporary Rust graph, return only generated MathML/canonical text/symbols, and never modify the document. The UI suppresses stale preview responses and hides old output while the source is a draft. Scalar domains are unspecified, real, nonnegative, positive and complex. Optional integer dimensions represent powers of mass/length/time. Unknown dimensions propagate as unknown; incompatible known additive dimensions fail. Fractional dimensions, units/conversions, matrix shapes and complete symbolic domain reasoning are not yet modeled.

The kernel conservatively declines unsupported transforms. Symbolic output is not a proof certificate. `actor: ai` is caller attribution, not authenticated identity. CLI callers have local file access already; there is no external server or trust elevation.

## Persistence and concurrency

Every mutation supplies the current revision. Validation precedes commit; conflicts and domain errors leave the document unchanged. Assumption changes invalidate current-branch outputs. Branches share immutable graph nodes but own distinct cells and assumptions. No merge semantics or named-cell dependency resolution is implied.

An advisory file lock excludes cooperating writers. Saving compares file bytes with the loaded baseline, then writes/syncs a same-directory temporary file, atomically renames it and syncs the directory on Unix. Files larger than 8 MiB are rejected. Invalid/version-mismatched documents are not silently replaced. This detects external changes but is not a distributed filesystem transaction.

Resource limits include 16,000 SEG nodes, 128 cells per branch, 16 branches, 64 assumptions, 2,048-byte source, parser depth 64 and 512 tokens, degree 32, 32 undo states, 256 audit events and 64 KiB CLI request lines. Nodes are not yet garbage-collected, so long sessions can reach the graph limit.

## Extension boundaries

New CAS, proof and simulation adapters should return explicit engine/version, assumptions, precision, trust and evidence, through the same transaction layer. Mature CAS/FLINT/Lean/Julia integration is intentionally deferred. An e-graph must carry conditional rewrite validity and bounded time/memory. GPU numeric acceleration must not inherit certified status from a CPU result automatically.

A future MCP gateway should invoke this core with explicit file/network grants; it should not automate the GUI. Content-addressed experiment blobs, complete audit replay and incremental dependencies belong in separate storage/runtime crates as they become substantial. The initial cohesive core avoids empty crates that imply nonexistent engines.
