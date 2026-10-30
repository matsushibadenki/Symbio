# MathJSON exchange, subset v1

Symbio now exchanges a bounded, exact subset of [MathJSON](https://mathlive.io/math-json/). This is an interoperability foundation, not a CortexJS runtime or complete MathJSON implementation. Parsing, context validation, source generation and calculation all run in Rust.

## GUI and CLI

In **Structure & history → MathJSON**, the package icon exports the selected cell's **authored definition**. Cell references are expanded to their definitions. The file contains a `symbio.mathjson` context envelope with the cell name, standard MathJSON expression and explicit domain/nonzero/MLT assumptions. It contains no cached results, calculation recipes, history, branch data or local paths. Use reproduction packages when those calculation records are needed.

The folder icon imports either a context envelope or a bare MathJSON expression into a **new untitled document**, then reveals its notebook. The current document and its assumptions remain separate. Bare expressions have no inferred real/complex domain assumptions. The imported source is generated LaTeX and is immediately calculated by the normal Rust kernel; domain/unsupported calculation errors remain visible in the cell. Imported results are Symbolic, not Verified.

```sh
cargo run --locked -p symbio-cli -- export-mathjson study.symbio expression.mathjson.json
cargo run --locked -p symbio-cli -- import-mathjson expression.mathjson.json new-study.symbio
```

CLI export uses the selected cell and requires the source document not to be locked by the desktop. Both commands refuse to overwrite existing destinations. GUI export also refuses replacement, and GUI import retains the original document in its Rust-owned session. Pending drafts must be applied before either operation; invalid drafts block the action. Native dialog operations use generation/revision checks and the existing file-operation guard.

## Supported expression forms

- Exact integer JSON numbers within signed/unsigned 64-bit range, integer strings and `{"num":"integer"}`. Larger integers use strings and never pass through float64.
- `Rational(integer, integer)` with a nonzero denominator.
- Scalar symbols, `Pi`, `ExponentialE`, `Add`, `Subtract`, `Multiply`, `Divide`, `Power`, `Negate`, binary `Equal`.
- Unary `Sin`, `Cos`, `Exp`, `Ln`, `Sqrt`, `Abs`, `Floor`, `Ceil`. `Ln` is natural logarithm; `Log` is deliberately not treated as its alias because the [standard distinguishes them](https://mathlive.io/compute-engine/reference/arithmetic/).
- Rectangular matrices up to 8×8, using [lists of lists](https://mathlive.io/compute-engine/reference/linear-algebra/), for example `["List",["List",1,2],["List",3,4]]`. Supported matrix products use `Multiply`, consistent with the standard. Vectors, arbitrary lists, Hadamard products and higher tensors are not mapped to matrices.
- Object forms `{"sym":"x"}` and `{"fn":["Add","x",1]}` with exactly one key.

User symbols use ASCII letters/digits/underscores, start with a letter or underscore and have at most 48 bytes. Multi-letter names starting with an uppercase letter are rejected unless explicitly supported constants, to avoid interpreting unknown standard-library objects as ordinary variables. Single uppercase physical symbols such as `E`, `M`, `L`, `T` are supported. Reserved built-in names are not silently redefined. The wider MathJSON language supports names outside this subset; they require future mappings.

## Context envelope

```json
{
  "format": "symbio.mathjson",
  "version": 1,
  "name": "model",
  "expression": ["Equal", "z", ["Subtract", ["Power", "x", 2], ["Power", "y", 2]]],
  "assumptions": {
    "x": {"domain": "real", "nonzero": false, "dimension": null},
    "y": {"domain": "real", "nonzero": false, "dimension": null}
  }
}
```

The envelope is Symbio-specific; a standard MathJSON consumer should use its `expression` member. The envelope preserves assumptions rather than pretending they are encoded by a bare expression. Source spelling and graph IDs can change; the mathematical object is reconstructed, not its original typography or shared-node identity. Exact integers/rationals can normalize signs, and `ExponentialE` can be represented as `Exp(1)` in generated source.

## Explicit limits

Fractional/exponential JSON number forms, approximate literals, infinities, complex constants, Unicode identifiers, metadata fields, arbitrary functions/function heads, calculus binders and solution sets are rejected. The adapter does not stringify unknown objects or evaluate supplied code. `Add`/`Multiply` require 2–128 operands; other mapped functions have fixed arity. Input is limited to 1 MiB, depth 48, an expanded conversion budget of 4096 nodes, 64 assumptions and the existing 2048-byte notebook-source limit. Export also checks source representability, so it does not knowingly create an expression too large for reimport.

OpenMath, calculus exchange, approximate-number provenance and full standard-library interoperability remain future work. `python3 scripts/smoke-mathjson.py` verifies CLI roundtrip, exact large numbers, assumptions and file preservation. Rust tests also cover matrix products, equations, reference expansion, unsupported inputs and no-clobber behavior.
