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

- Exact integer JSON numbers, integer strings and `{"num":"integer"}`. Integer literals never pass through float64.
- Decimal and exponent literals in JSON numbers, numeric strings or `{"num":"…"}` are stored as approximate SEG nodes. Spelling and trailing zeros survive notebook storage and MathJSON export. The Symbio extension `{"num":"1.2300","precision":5}` carries a declared significant-digit count (1–4096). An integral spelling with explicit precision also denotes an approximate value. Export uses number objects and preserves omitted precision.
- `Rational(integer, integer)` with a nonzero denominator.
- Scalar symbols, `Pi`, `ExponentialE`, `Add`, `Subtract`, `Multiply`, `Divide`, `Power`, `Negate`, binary `Equal`.
- Unary `Sin`, `Cos`, `Exp`, `Ln`, `Sqrt`, `Abs`, `Floor`, `Ceil`. `Ln` is natural logarithm; `Log` is deliberately not treated as its alias because the [standard distinguishes them](https://mathlive.io/compute-engine/reference/arithmetic/).
- Rectangular matrices up to 8×8, using [lists of lists](https://mathlive.io/compute-engine/reference/linear-algebra/), for example `["List",["List",1,2],["List",3,4]]`. Matrix-by-matrix products export as explicit binary `MatrixMultiply(A, B)`. Both operands must be compatible matrices; scalar scaling continues to use `Multiply`, and existing `Multiply` matrix inputs remain accepted. Operand order is preserved. Vectors, arbitrary lists, Hadamard products and higher tensors are not mapped to matrices.
- Unary `Determinant(matrix)`, `Transpose(matrix)` and `Inverse(matrix)` preserve authored matrix operators and run the existing exact Rust matrix kernel. Determinants and inverses require a square matrix; transposes accept rectangular matrices. A singular inverse remains a visible domain error. Inverse notation may normalize to a matrix power of −1 through generated LaTeX; the original operation is retained mathematically, not byte-for-byte. Only explicit compatible matrices (up to 8×8) are accepted; scalar, vector, tensor-axis and conjugate-transpose extensions are outside this subset. See the [linear algebra reference](https://mathlive.io/compute-engine/reference/linear-algebra/).
- Explicit calculus binders: `D(body, variable)`, `Integrate(body, variable)`, `Integrate(body, Tuple(variable, lower, upper))`, and `Sum`/`Product(body, Tuple(variable, lower, upper))`. These follow the [CortexJS calculus](https://mathlive.io/compute-engine/reference/calculus/) and [sum/product](https://mathlive.io/compute-engine/reference/arithmetic/) forms. Each head accepts one variable/limit specification; use nested expressions for multiple operations. Variables are explicit strings or single-key `sym` objects. Bound bodies and limits remain distinct SEG children.
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

Repeating decimals, infinities, complex constants, Unicode identifiers, metadata fields, arbitrary functions/function heads, implicit-variable calculus, functional `Derivative` and solution sets are rejected. The adapter does not stringify unknown objects or evaluate supplied code. `Add`/`Multiply` require 2–128 operands; other mapped functions have fixed arity. Input is limited to 1 MiB, depth 48, an expanded conversion budget of 4096 nodes, 64 assumptions and the existing 2048-byte notebook-source limit. Export also checks source representability, so it does not knowingly create an expression too large for reimport.

A bounded [OpenMath XML phrasebook](openmath.md) is now available. Wider calculus syntax, full standard-library interoperability remain future work. `python3 scripts/smoke-mathjson.py` verifies CLI roundtrip, exact large numbers, assumptions and file preservation. Rust tests also cover matrix products, equations, reference expansion, unsupported inputs and no-clobber behavior.

## Calculus examples and evaluation scope

```json
["Integrate", ["Power", "x", 2], ["Tuple", "x", 0, 2]]
```

Imports as a definite integral, displays its integral notation and evaluates to the exact rational `8/3`. Export retains `Integrate` and its limits, rather than replacing the definition with that cached number. Likewise `["D", ["Power", "x", 3], "x"]` maps to a first derivative, `["Sum", ["Power", "k", 2], ["Tuple", "k", 1, 3]]` evaluates to `14`, and `["Product", "k", ["Tuple", "k", 1, 4]]` evaluates to `24`. See [the example context](../examples/calculus.mathjson.json).

The codec preserves authored operators, nested scope, free parameters in bounds and explicit context assumptions. It does not expand the kernel's calculus algorithms. Unsupported symbolic primitives or symbolic/noninteger summation limits remain visible as calculation errors while their valid definitions remain exchangeable. Aggregates use the existing maximum of 256 integer terms; resource-limit violations can reject import before a document is created. Indefinite results retain the existing condition to add an arbitrary constant C. This is symbolic computation, not formal proof.

Omitted variables, partial tuples, more than one limit argument per head, infinite bounds, higher-order count syntax, function literals, `ND` and `NIntegrate` are rejected. Higher derivatives can be represented with nested `D`. Plain `"D"` remains an ordinary physical symbol, distinct from the array-headed operator. No variable is inferred from a name or from LaTeX spelling. The envelope version remains 1: existing exact expressions retain their meanings and the supported subset grows additively.

Approximate literals use the editable notebook notation `\symbioapprox{1.2300:5}` and display with an approximation marker. Symbolic evaluation keeps the authored expression with approximate trust. Use numeric evaluation for float64 computation. The declared digit count describes the input and supplies no error bound or arbitrary-precision computation. Decimal lexemes are limited to 1024 bytes and subject to the notebook source limit.

Named symbolic applications can be exchanged as `["Apply", {"sym":"f"}, "x"]`. The explicit symbol identifies the function and the remaining entries are its ordered scalar arguments. Nested applications are supported.

`Trace` takes one square matrix and sums its diagonal entries. `Rank` takes one rectangular matrix and evaluates the rank by exact rational elimination when its entries reduce to rationals. Both operator heads survive OpenMath exchange.

`RowCount` and `ColumnCount` each take one matrix expression and return its row or column count as an exact integer. Exchange preserves the operation and its operand, including nested expressions such as `Transpose`.

`["MatrixSelector", row, column, matrix]` selects a matrix entry using one-based integer indices. The exchange preserves both indices and the matrix expression. Invalid indices produce a domain error.

`["VectorSelector", index, ["Vector", ...entries]]` selects a scalar entry from an explicit vector of up to 32 entries. Indices are one-based integers. OpenMath exchange preserves the selector, index and entries. Invalid indices produce a domain error.

`["ScalarProduct", ["Vector", ...left], ["Vector", ...right]]` computes the sum of pairwise products. OpenMath exchange preserves the operand order and entries. Each explicit vector can contain up to 32 scalar entries. Different lengths produce a domain error.

`["VectorProduct", ["Vector", ...left], ["Vector", ...right]]` preserves the operand order for a three-dimensional cross product. The kernel displays the three result components as a row matrix. Other operand lengths yield a domain error.

`["OuterProduct", ["Vector", ...left], ["Vector", ...right]]` preserves both ordered vectors and produces a matrix whose row count is the left vector length and column count is the right vector length. Each vector must have 1–8 scalar entries.

`["MatrixTensor", A, B, ...]` preserves the ordered operands of a matrix Kronecker product. There must be 2–32 matrix operands, and each resulting dimension must be at most 8.

`["VectorTensor", ["Vector", ...left], ["Vector", ...right], ...]` preserves the ordered explicit vectors in a Kronecker product. It accepts 2–32 nonempty vectors whose lengths multiply to at most 8. The result is displayed as a single row.

`["IdentityMatrix", n]` constructs an exact square identity matrix for an integer literal of order 1–8. OpenMath exchange preserves the constructor and its order. Invalid orders produce a domain error at import.

`["ZeroMatrix", rows, columns]` constructs an exact zero matrix and preserves its ordered dimensions during OpenMath exchange. Both dimensions must be integers from 1 to 8.

`["ConstantMatrix", n, value]` constructs a square matrix of order 1–8 with the same scalar in every entry. OpenMath exchange preserves the constructor, order and scalar expression.

`["DiagonalMatrix", a, b, ...]` constructs a square matrix from 1–8 ordered diagonal entries. It preserves the constructor and entry order during OpenMath exchange.

`["ScalarMatrix", n, value]` constructs a scalar multiple of the identity of order 1–8 and preserves the constructor and both arguments during OpenMath exchange.
