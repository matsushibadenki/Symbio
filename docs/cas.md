# Optional local CAS

Symbio can factor exact rational univariate polynomials using a locally installed SymPy. The Rust kernel remains the document and calculation authority. It also finds exact real polynomial roots when the rational factorization consists entirely of linear and quadratic factors. General integration, unrestricted equation solving, multivariate factorization, algebraic coefficient fields and formal proof are not implemented by this adapter.

## Configuration

In **Structure & history → CAS · SymPy**, enter the absolute path to a Python interpreter that already has SymPy, then apply. Apply probes the interpreter before atomically saving the application preference. Disable clears the runtime selection. No packages are installed automatically. Opening a research document does not select an executable or launch Python. Configuration is application-wide and is not included in documents or reproduction packages. Startup restores the path without probing; apply again to display its installed version.

For the CLI, set `SYMBIO_CAS_PYTHON` to an absolute interpreter path. Preserve a virtual environment's executable path: resolving its symlink can change its package environment.

```sh
SYMBIO_CAS_PYTHON=/absolute/venv/bin/python3 cargo run --locked -p symbio-cli -- serve study.symbio
```

Use `math.transform` with `operation: "factor"` and `variable: "x"`, or the notebook's `(·)(·)` control. For an explicit equation use `operation: "solve_polynomial"`, or its separate `x = ? · CAS` control. The same saved recipe runs after source or dependency changes. Failure clears stale dependent outputs through the existing replay mechanism. There is no silent fallback to another engine. Cached results remain readable without Python.

## Mathematical scope and evidence

Inputs must be polynomials in the selected variable over ℚ, degree at most 32, with numerator and denominator sizes at most 256 bits. Other symbols and variable denominators are rejected before external execution; `x/x` is not canceled first. Constants and zero are supported. The adapter uses SymPy's [`Poly.factor_list`](https://docs.sympy.org/latest/modules/polys/reference.html), preserving rational content and repeated factors.

Only rational coefficient vectors cross the boundary. Returned factors are rebuilt as SEG nodes, independently expanded by Rust exact arithmetic, and compared coefficient by coefficient with the original polynomial. Mismatches, malformed output and excess bounds are rejected transactionally. This checks the product identity; it does **not** independently establish irreducibility or constitute a formal proof. Results remain **Symbolic**, with the actual SymPy version, Rust engine identifier, scope and identity check recorded in output metadata.

## Execution boundary

An embedded, fixed Python worker runs directly with `-I -B`; no shell, user source evaluation, `sympify`, arbitrary function names or document-selected executable paths are used. Each call has an eight-second wall limit, best-effort five-second CPU limit on supported Unix systems, bounded protocol input/output and strict response decoding. This is a bounded local worker, not an operating-system sandbox. Only configure a trusted interpreter and trusted installed SymPy package. Large arbitrary-precision or general CAS jobs are outside this contract.

Reproduction verification skips factor and polynomial-solve recipes by default, reporting `external_runtime`. To explicitly run the already configured local runtime, enable **Allow external CAS during replay**, or use:

```sh
SYMBIO_CAS_PYTHON=/absolute/venv/bin/python3 cargo run --locked -p symbio-cli -- verify-reproduction study.symbio-repro.json --allow-cas
```

A package cannot supply a Python path. Replay still compares result structure, conditions and engine metadata; a changed SymPy version may be reported as a difference. `python3 scripts/smoke-cas.py` exercises the real local adapter and requires SymPy in `SYMBIO_CAS_PYTHON` or the script's own interpreter.

## Exact real polynomial solving

`solve_polynomial` takes an explicit equation, builds its left-minus-right polynomial **before** simplification, and uses the existing bounded factor worker. Rust independently expands the factor product and compares every rational coefficient with the original difference. It then solves every nonconstant factor with the Rust linear/quadratic solver, using monic normalization and deduplicating root nodes. Repeated factors return roots once, not multiplicities. Radical results remain exact SEG expressions; results are Symbolic, not Verified. The output records the actual SymPy version and Rust engine `symbio-rust/0.7.0`.

For example, enter LaTeX `x^6=1` and choose `x = ? · CAS`. The real solution set is `{−1, 1}`; quadratic factors with negative discriminants contribute no real roots. `x^4-5x^2+6=0` returns four exact radical roots. `x^3=x` includes the zero root. Persisted solution sets now allow up to 32 entries, matching the maximum polynomial degree. The existing offline `x = ?` operation remains limited to degree two and needs no Python.

If any factor has degree above two, the **entire solve fails** with `unsupported`; roots from the remaining factors are not presented as a complete answer. Thus `x^8=1` and `(x-1)(x^3-x-1)=0` are rejected by this adapter despite having real roots. This scope can depend on factorization granularity; it is not a general real-root isolation algorithm. The coefficient identity proves factor-product equality within the arithmetic model, not formal verification of the full solver.

Rational constants, no-real-root cases, nonzero/sign assumptions and complex-domain rejection follow the existing Rust solver contract. A nonzero constant equation returns the empty set; an identity has infinitely many solutions and is rejected rather than returning an empty set. Filtering irrational roots under sign/nonzero assumptions is still unsupported and rejects the whole solve. Other variables, variable denominators (`x/x=1`), singular rational functions and non-polynomial operators are not silently simplified into the supported domain.

Recipes retain source, assumptions, selected variable and engine evidence. Source/reference/assumption edits recalculate them, unsupported results clear stale outputs, and Undo/Redo and save/reload keep the existing semantics. Cached results remain readable without Python; rerunning requires the configured runtime. Reproduction packages require explicit external-CAS permission for this operation, just as for factorization. The stateless MCP adapter does not expose it.

`python3 scripts/smoke-cas-solve.py` checks the real optional runtime, exact roots, rejection of partial answers, transactional failure, dependency replay, Undo/Redo, persistence and default/opt-in reproduction. See [a MathJSON equation example](../examples/cas-polynomial.mathjson.json), which can be imported as an independent notebook document.
