# Calculation reproduction packages

A `.symbio-repro.json` file captures the **active branch** as readable JSON: original Expression/LaTeX source, stable cell identities and names, notes, explicit assumptions, saved calculation recipes, source document/branch/revision, engine identifier, a bounded exporter kernel-environment manifest, and expected result evidence. All cells in that branch are included so references remain self-contained. Other branches, the audit tail, Undo/Redo, local paths and GPU data are excluded.

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

Float64 comparison has no tolerance or certified error bound. Other platforms or library versions may produce differences even for reasonable approximations. The engine identifier is a compatibility label, not a binary/build hash or a complete operating-system manifest. By default no external software, network access, plugins or shell commands are run from a package. External factor and `solve_polynomial` recipes are skipped with `external_runtime`. Explicit `--allow-cas` (or the GUI checkbox) permits the locally configured SymPy worker; packages never select executable paths. See [the CAS contract](cas.md).

Inputs are bounded to 8 MiB, 128 cells, 64 assumptions and existing parser/SEG/calculation limits. Unknown fields, invalid references/cycles, unsupported versions, malformed evidence and checksum mismatches fail before replay. Replay cannot promise a wall-clock deadline for every bounded symbolic problem; large-package and long-session performance remains a validation task.

Known domain annotations from early documents are normalized between lowercase and enum-style capitalization before evidence comparison. Other condition text is preserved. The report includes recorded and replayed evidence, so an older per-result engine is visible even when the package itself was exported with the current engine. Large expression previews are omitted using a bounded expanded-size estimate; their structural digests are still checked without expanding DAG sharing.

## Kernel environment manifest (optional v1 field)

New packages add `payload.environment`, covered by the payload SHA-256. It describes the **exporting build**, not necessarily the historical environment that computed every cached result. Fields are manifest version 1, build-target OS and CPU architecture, pointer width, byte order, core Cargo package version, the exact-rational/uncertified-binary64 numeric policy, and SHA-256 of the Cargo.lock bytes embedded when the core was compiled. The lockfile digest is a dependency specification fingerprint, not verification of the installed dependencies or a hash of source code, vendored source, binary, compiler, GPU driver or linked native libraries.

Verification captures the same fields locally and returns `recorded_environment`, `current_environment`, `environment_status` (`matched`, `different`, `unrecorded`) and `environment_differences`. The GUI shows the status separately and an expandable exporter/verifier comparison table. Differing rows are highlighted. Environment agreement does not establish mathematical correctness, binary identity or bitwise portability. A foreign environment can still yield matching results; environment differences do not change the existing result-based `reproduced` flag or CLI exit behavior. Inspect both the replay report and environment status.

Existing v1 packages without the optional field retain their checksum serialization and remain readable; the status is `unrecorded`, never an assumed match. Unsupported manifest versions, unknown fields, malformed digests and excessive metadata are rejected before replay. There is no automatic installation, environment restoration, external runtime probing, path selection or execution based on a manifest. Hostnames, usernames, environment variables and interpreter paths are not collected. SymPy versions used for results remain in per-result engine evidence; the exporter manifest does not assert which interpreter or library produced an older cached result.

Complete event replay and original-per-calculation environment capture remain planned. This manifest does not change the document format or engine identifier.

### Compiler identity

New environment manifests include an optional `compiler` field captured with `RUSTC --version` at build time. No local executable is probed when opening or verifying a package. Older manifests deserialize with this field absent; missing versus recorded compiler identity is reported as an environment difference, independently of mathematical replay success. This field does not fingerprint the compiled binary or recover its historical dependencies.

### Core source identity

New manifests include optional `core_source_sha256`. The build hashes a versioned, length-framed stream of sorted relative names and raw file bytes: the core Cargo.toml, build.rs and every .rs and .py file under src (including the embedded CAS worker). Absolute paths, Git metadata, tests, UI, external interpreter/library contents and binary contents are excluded. The stream prefix is now `symbio-core-source-v2`, so older v1 digests intentionally compare as different. The embedded Python worker is part of this source set; the installed SymPy package remains separate per-result evidence. Source symlinks fail the build rather than silently escaping this scope. Cargo tracks source-directory changes so added/deleted files trigger rebuilding. This identifies a bounded source set, not the whole application or historical execution environment. Missing legacy fingerprints compare as differences without preventing mathematical replay.

### Selected build configuration

The optional `build` object records the core's target triple, Cargo optimization level, effective debug-assertion mode and target CPU feature list. Target, optimization and feature values are captured by the build script; assertion mode is compiled into the core. Missing legacy data and any changed setting are reported as a `build` environment difference without changing mathematical replay status. Inputs are bounded and validated. Raw RUSTFLAGS, compiler paths, environment variables, credentials and hostnames are not included. This is a selected configuration record: linker options, LTO, dependency-specific profiles, native libraries and the final binary are not fully identified.
