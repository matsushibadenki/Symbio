# Mathematical contract

Symbio is a reproducible derivation notebook with a bounded Rust symbolic kernel. It is not a complete Mathematica implementation. External CAS adapters are planned; SymPy currently serves only as an optional development oracle.

## Operations and evidence

- **Symbolic** reduces the expression using explicit assumptions. Parameter input fields do not implicitly change this operation.
- **Substitute** replaces selected free variables with exact integer, decimal or fraction literals, then reduces the result. Empty fields remain symbolic: substituting `x = 2` in `z = x^2-y^2` gives `z = 4-y^2`. Decimal `0.1` means the rational `1/10`. This is a Symbolic result, not a formal proof.
- **Numeric** requires finite numeric bindings for input parameters and returns float64 approximations without certified error bounds. For `z = f(x,y)`, `z` can remain the output label; `x=2,y=3` gives `z ≈ -5` for the example above. Instantiating a general equation is not a Boolean verification of that equation.

Exact substitution is applied before cancellation so singular inputs cannot disappear behind `x/x -> 1`. Unknown variables, zero denominators and explicit sign/nonzero assumption violations fail atomically. Dimensioned substitutions currently require unsupported quantity values and are rejected rather than losing their units. Reduce a derivative or indefinite integral before substituting its differentiation/integration variable; unsupported operator ordering is rejected.

Saved calculation recipes include exact bindings. Expression, reference and assumption edits replay dependent recipes; Undo/Redo and save/reload preserve them. History records operations and conditions, but does not provide a formal derivation certificate or a complete proof trace.

## Domains and model roles

An unconfigured variable is **Unspecified**, with no implicit real or complex assumption. The UI lists unconfigured input parameters without silently adding assumptions. Under a real assumption `sqrt(x^2)` can become `abs(x)`; under a nonnegative assumption it can become `x`. General complex expressions retain branch-sensitive forms when no supported rule applies.

Division by zero and `0^0` are domain errors. Numeric evaluation accepts finite values; there is no distinguished infinity object (`Infinity` is an ordinary identifier). Complex symbolic support is limited; analytic continuation, general branch cuts and complex numerical arithmetic are not certified.

For an explicit equation with a single symbol on the left absent from the right, the UI identifies that symbol as the model output. Thus `z = x^2-y^2` has inputs `x,y` and output `z`. This presentation metadata does not remove `z` from the equation's algebraic free symbols. Implicit and self-dependent equations are not given a false output role.

## Input interpretation

Expression mode requires explicit multiplication and treats `ab` as one identifier. In LaTeX mode plain `ab` is multiplication; `\mathit{ab}` denotes one identifier. General user-defined function calls are not implemented: plain LaTeX `f(x)` is multiplication, while supported built-ins use forms such as `\sin(x)` or `\sin{x}`. Ungrouped ambiguous function arguments can be rejected.

Integral syntax recognizes its differential and bound variable. Outside that syntax plain `dx` is multiplication in LaTeX mode. Subscripts name symbols; they do not implement tensor contraction. Source references remain explicit (`@a` or `\cell{a}`); rendered references use a dotted link. `a := expression` labels a notebook definition, not a general function declaration.

## Current limits and validation

The runtime uses exact rational arithmetic and a limited set of symbolic, matrix, integral and equation algorithms. Arbitrary precision floating point, interval arithmetic, general integration, tensor/noncommutative algebra, formal proof checking and scientific figure export remain future work. Engine errors are preferable to guessed results.

`cargo test -p symbio-core -p symbio-desktop --offline` includes regression tests for partial substitution, singularities, assumptions, references, recipe replay, equations and calculus ordering. `python3 scripts/smoke-research-contract.py` optionally checks six bounded cases against locally installed SymPy. This is not a general CAS benchmark, Rubi conformance result or validation of the advanced physics problems proposed in the feedback.
