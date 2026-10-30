# Calculation reproduction packages

A `.symbio-repro.json` file captures the **active branch** as readable JSON: original Expression/LaTeX source, stable cell identities and names, notes, explicit assumptions, saved calculation recipes, source document/branch/revision, engine identifier, and expected result evidence. All cells in that branch are included so references remain self-contained. Other branches, the audit tail, Undo/Redo, local paths and GPU data are excluded.

## Use

In **Structure & history → Reproduction**, use the package icon to export the active branch through a native Save dialog. Pending source/note drafts are applied first; invalid drafts block export. Choose a new filename: existing documents, packages and symlink targets are never overwritten. Export does not change the active save location, mark the document saved, switch branches or append an audit event.

The replay icon opens a package through a native file picker and shows a per-cell report in the same panel. Verification runs in Rust on a separate fresh project and does not load that project into the editor or modify the open document. Expand a report row to inspect its recomputed result or differences. The report labels the package's document, branch and revision, independently of the open document.

CLI equivalents:

```sh
cargo run --locked -p symbio-cli -- export-reproduction /path/study.symbio /path/study.symbio-repro.json
cargo run --locked -p symbio-cli -- verify-reproduction /path/study.symbio-repro.json
```

The source project must not be locked by another process when exporting through the CLI. Verification needs only the package, not the original file. It prints JSON and exits nonzero on incomplete replay, differences, integrity failure or invalid input. It never writes to the package or source document. `python3 scripts/smoke-reproduction.py` exercises this workflow.

## What is checked

The verifier validates the format version, size and SHA-256 payload checksum, parses every source into a fresh SEG, validates references and calculation recipes, and executes the recorded operations in dependency order through the same Rust kernel used by the application. Captured result caches and audit commands are not used as calculation inputs. The current reference semantics expand authored definitions, not another cell's cached output.

Each result compares:

- A structural SHA-256 expression digest, independent of SEG node IDs and garbage collection.
- Exact IEEE-754 bits for scalar float64 values. Numeric matrices and equations are compared through their resulting SEG structure.
- Operation, trust classification, precision, applicable conditions and engine metadata.
- Recorded error code versus the replayed error code, separately from successful results.

`matched` means a recorded result and its evidence matched. `error_reproduced` means the same error code occurred; it does not mean a valid mathematical result was obtained. Missing recipes or missing expectations are `skipped`; empty packages are never reported as fully reproduced. `mismatch` lists changed fields. The overall `reproduced` flag requires all cells to match a result or error and the package engine identifier to match the current engine.

## Boundaries

This is **current-branch recipe replay**, not complete historical event replay, independent CAS validation, a digital signature or a formal proof. Anyone can recompute the checksum; it detects accidental changes, not trusted authorship. Matching identical erroneous kernels does not prove mathematical correctness. Structurally different but mathematically equivalent expressions may be reported as differences.

Float64 comparison has no tolerance or certified error bound. Other platforms or library versions may produce differences even for reasonable approximations. The engine identifier is a compatibility label, not a binary/build hash or a complete operating-system manifest. No external software, network access, plugins or shell commands are run from a package. The current kernel has no external runtime dependencies to bundle.

Inputs are bounded to 8 MiB, 128 cells, 64 assumptions and existing parser/SEG/calculation limits. Unknown fields, invalid references/cycles, unsupported versions, malformed evidence and checksum mismatches fail before replay. Replay cannot promise a wall-clock deadline for every bounded symbolic problem; large-package and long-session performance remains a validation task.

Known domain annotations from early documents are normalized between lowercase and enum-style capitalization before evidence comparison. Other condition text is preserved. The report includes recorded and replayed evidence, so an older per-result engine is visible even when the package itself was exported with the current engine. Large expression previews are omitted using a bounded expanded-size estimate; their structural digests are still checked without expanding DAG sharing.
