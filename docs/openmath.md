# Exact OpenMath XML exchange

Symbio provides a bounded OpenMath 2 XML phrasebook, implemented in Rust through the existing SEG, exact MathJSON mapping and generated LaTeX source. This is partial exchange support, not an unrestricted OpenMath implementation or an external CAS runtime.

## Use

In **Structure & history → OpenMath**, export the selected cell's authored definition, or import an XML file into a separate new untitled document. Export resolves cell references and retains the source cell's name and explicit assumptions. It excludes cached results, recipes, history, other branches, local paths and GPU data. Import uses the normal Rust calculation flow; unsupported mathematical evaluation remains visible as a cell error when the definition itself is supported.

```sh
cargo run --locked --offline -p symbio-cli -- export-openmath study.symbio expression.openmath.xml
cargo run --locked --offline -p symbio-cli -- import-openmath expression.openmath.xml new-study.symbio
```

Both commands and the GUI refuse to overwrite existing destinations. GUI exchange retains draft, file-operation and document generation/revision guards. Plain OpenMath objects have no inferred domain assumptions. See [the exact-number example](../examples/exact.openmath.xml), [the definite-integral example](../examples/calculus.openmath.xml) and [the rectangular transpose example](../examples/transpose.openmath.xml).

## Supported meanings

Mappings follow the [OpenMath XML standard](https://openmath.org/standard/om20-2019-07-01/omstd20.html) and its content dictionaries:

| OpenMath object / symbol | Symbio meaning |
| --- | --- |
| `OMI` | Exact decimal integer, including integers larger than float64 |
| `OMV name` | A supported ASCII scalar symbol |
| `nums1.rational`, `nums1.pi`, `nums1.e` | Exact rational constructor and named constants |
| `arith1.plus`, `minus`, `times`, `divide`, `power`, `unary_minus`, `abs` | Scalar arithmetic |
| `arith1.plus`, `arith1.minus` with matrix operands | Entrywise addition and subtraction of equal-shaped matrices |
| [`arith1.times`](https://openmath.org/cd/arith1.html#times) | Ordered matrix products and scalar multiplication of matrices. Matrix dimensions are checked before evaluation. |
| `arith1.root(expression, 2)` | Principal square root |
| `transc1.sin`, `cos`, `exp`, `ln` | Supported unary functions; `ln` is natural logarithm |
| `rounding1.floor`, `ceiling` | Existing floor/ceiling operators |
| `calculus1.diff(lambda(x, body))(x)` | Explicit expression derivative with respect to `x` |
| `calculus1.int(lambda(x, body))(x)` | Indefinite primitive, with the kernel's arbitrary-constant condition |
| `calculus1.defint(oriented_interval(a, b), lambda(x, body))` | Definite integral with ordered start/end values |
| `arith1.sum` / `product` over `interval1.integer_interval(a, b)` and a unary lambda | Inclusive finite sum/product |
| `linalg1.determinant` | Exact determinant of a square matrix |
| `arith1.divide(matrix, scalar)` | Exact entrywise division by a scalar. A zero divisor retains the definition and reports a domain error. |
| `arith1.power(matrix, -1)` | Inverse of a square matrix, exchanged as MathJSON `Inverse`. Singular matrices retain the definition and report a domain error. |
| `arith1.power(matrix, exponent)` | Powers of square matrices. The kernel evaluates integer exponents from −16 to 16, including zero. |
| `linalg4.rowcount`, `linalg4.columncount` | Exact row and column counts of a matrix |
| `linalg4.rank` | Exact rank of a rectangular matrix with rational entries |
| `symbio_linalg1.trace` | Sum of the diagonal entries of a square matrix. Requires the [Symbio dictionary](openmath-cd/symbio_linalg1.ocd) and its explicit `urn:symbio:cd` base. |
| `linalg5.diagonal_matrix` | Square matrix with 1–8 ordered scalar diagonal entries and zeros elsewhere. |
| `linalg5.scalar(n, value)` | Scalar multiple of the identity of order 1–8. |
| `linalg5.constant(n, value)` | Square matrix of order 1–8 with the same scalar in every entry. |
| `linalg5.zero(rows, columns)` | Exact rectangular zero matrix. Both dimensions are integers from 1 to 8. |
| `linalg5.identity(n)` | Exact square identity matrix of order 1–8. Invalid orders produce a domain error at import. |
| `linalg6.vector_tensor` | Ordered Kronecker product of 2–32 explicit row vectors. The result is displayed as a single row of up to 8 entries. |
| `linalg6.matrix_tensor` | Ordered matrix Kronecker product using SymPy block order. Accepts 2–32 operands, with each result dimension limited to 8. |
| `linalg1.outerproduct(left, right)` | Matrix with entries `left[i] * right[j]`. Explicit vector lengths may differ and each must be 1–8. |
| `linalg1.vectorproduct(left, right)` | Ordered cross product of two explicit three-component `linalg2.vector` operands. Symbio displays the result as a 1×3 row. |
| `linalg1.scalarproduct(left, right)` | Dot product of two equal-length explicit `linalg2.vector` operands, up to 32 scalar entries each. Different lengths produce a domain error. |
| `linalg1.vector_selector(index, vector)` | Exact entry from an explicit `linalg2.vector` with up to 32 scalar entries, using a one-based integer index |
| `linalg1.matrix_selector(row, column, matrix)` | Exact matrix entry at one-based integer indices. Out-of-range indices report a domain error. |
| `linalg1.transpose` | Transpose of a rectangular matrix |
| `linalg3.matrix` with `matrixcolumn` children | Column-oriented input converted to the same row-oriented SEG matrix |
| `relation1.eq` | Binary equality |
| `linalg2.matrix` containing `linalg2.matrixrow` | Row-oriented rectangular matrix constructor, up to 8×8 |

See [arith1](https://openmath.org/cd/arith1.html), [nums1](https://openmath.org/cd/nums1.html), [transc1](https://openmath.org/cd/transc1.html), [rounding1](https://openmath.org/cd/rounding1.html) and [linalg2](https://openmath.org/cd/linalg2.html). Matrix entries must be scalar. Matrix equality and the matrix operations listed above use the kernel’s existing shape checks. Powers with exponents outside the evaluation range retain the authored expression and report a calculation error. The matrix operators follow [linalg1](https://openmath.org/cd/linalg1.html); vector transpose is not supported. Operators survive exchange as authored SEG calls, not just evaluated entries. `plus`/`times` require 2–128 operands; the other supported operators have fixed arity. General nth roots are not mapped to square roots.

The accepted namespace is `http://www.openmath.org/OpenMath`; default namespaces and bound prefixes both work. Root `version` may be omitted or `2.0`. An optional root or symbol `cdbase` may explicitly identify `http://www.openmath.org/cd`. Other inherited dictionary bases are rejected rather than interpreting a foreign symbol as an official one. The context extension below has its own explicit base. Namespace shadowing cannot change a foreign element into a supported object merely by sharing its local name.

## Explicit calculus and scope

The calculus forms follow [calculus1](https://openmath.org/cd/calculus1.html), unary [fns1.lambda](https://openmath.org/cd/fns1.html), [interval1](https://openmath.org/cd/interval1.html) and [arith1](https://openmath.org/cd/arith1.html). Each lambda uses `OMBIND`, an official `fns1.lambda` head, exactly one `OMV` in `OMBVAR`, and an explicit body. Bounds occur outside that lambda. Nested binders, shadowed indices and free parameters use the existing scoped SEG representation.

`diff` and `int` produce functions, so Symbio requires the outer application at the **same named variable**. Bare functions, application at another symbol or numeric point, annotated/multiple bound variables and partial-derivative operators are rejected. No variable inference or alpha-renaming is attempted. Definite integrals accept only `interval1.oriented_interval`; closed/open intervals and the undeclared `ordered_interval` spelling are not aliases. Reversed limits retain their sign. Sums/products accept only `integer_interval`; empty integer ranges use the existing kernel's zero/one identities.

The exchanged object is always the authored operator, never a cached answer. Existing exact evaluation and bounded sums/products apply. Unsupported integrands remain present with an evaluation error; singular intervals do not become successful answers. This adds no general integration algorithm, function type or certified numerical integration.

## Symbio semantic context

Export wraps the expression in one standard `OMATTR`/`OMATP` attribution:

```xml
<OMATTR>
  <OMATP>
    <OMS cdbase="urn:symbio:cd" cd="symbio_context1" name="context"/>
    <OMSTR>{"version":1,"name":"f","assumptions":{"x":{"domain":"real","nonzero":false,"dimension":null}}}</OMSTR>
  </OMATP>
  <!-- authored expression -->
</OMATTR>
```

The [local experimental dictionary](openmath-cd/symbio_context1.ocd) defines `context` with the **semantic-attribution** role, because domain/nonzero/dimension assumptions affect permitted transformations. This is a Symbio extension, not an official OpenMath Society dictionary. A consumer must understand and preserve this context or reject the attributed object; simply stripping it can change the mathematical interpretation. The contained cell label itself has no mathematical effect. The dictionary URI is an identifier and is never fetched.

The OMSTR contains a JSON object with exactly `version`, `name` and `assumptions`. Version is 1; names and assumption keys follow the existing ASCII symbol rules (at most 48 bytes). At most 64 assumptions are accepted. Domains are `unspecified`, `real`, `nonnegative`, `positive` or `complex`; `nonzero` is Boolean, and `dimension` is null or the existing bounded three-component MLT vector. Core validation remains authoritative. Unknown context fields, versions or attribution keys are rejected. Nested/foreign annotations are not silently discarded.

Source spelling, negative-number syntax and `e` versus `exp(1)` may normalize through generated LaTeX. Exchange preserves supported mathematical meaning and explicit assumptions, not byte identity, source typography or original DAG IDs. Output retains Symbolic/Approximate distinctions from the normal calculation kernel and does not imply formal proof.

## Limits and validation

Input/output are bounded to 1 MiB, XML stack depth to at most 49 elements, and element count to 4096. Existing exact conversion budgets, 8×8 matrices, source-length limit of 2048 UTF-8 bytes and parser/graph/algorithm bounds also apply. Integer literals accept decimal digits with an optional sign and surrounding whitespace, with a 4096-digit parsing bound before the stricter source limit. Output uses UTF-8 XML 1.0 and XML escaping; supported character references are decoded once.

DTD, external entities, processing instructions, CDATA, `OMR` references, `OMFOREIGN`, approximate `OMF`, nondecimal integer encodings, unknown dictionaries and unsupported function heads, unknown attributes, standalone vector objects, general/standalone function binders, solution sets and other matrix arithmetic are rejected. No XML-related file/network lookup, TeX execution or external CAS is performed. Root/object sharing IDs and `xml:base` are not accepted. XML declarations, comments and formatting whitespace do not carry instructions to the application.

`python3 scripts/smoke-openmath.py` exercises real CLI/XML roundtrips, exact large rationals, context retention, immutable sources, no-clobber writes and DTD rejection. Core tests also cover namespace resolution/shadowing, unknown semantic data, resource limits, equations, matrices, calculus binding/scope, arbitrary-constant conditions and domain errors. The external codec roundtrip below covers the documented cases. The optional SymPy comparison below evaluates the matrix fixtures.

## Column-oriented matrix input

The experimental [linalg3 dictionary](https://openmath.org/cd/linalg3.html) describes matrices as columns. Symbio accepts nonempty rectangular matrices built from its matrixcolumn constructor, preserving entry order and shape during conversion to SEG. The existing 8×8 bound applies. Export uses the supported linalg2 row representation. Mixed row/column constructors, empty columns, ragged columns and standalone vectors are rejected. Existing determinant and transpose operators can wrap either supported matrix representation. Third-party application exchange has not been tested.

Named symbolic calls use an `OMA` headed by an `OMV`, with up to 32 ordered scalar arguments. Nested calls and zero-argument calls are supported. MathJSON uses `["Apply", {"sym":"f"}, "x"]` for a named call. Names follow the notebook ASCII identifier rules. Calls are retained as symbolic expressions for exchange.

## External codec interoperability

`scripts/check-openmath-interop.py` exercises 49 cases through [py-openmath 0.3.0](https://github.com/OpenMath/py-openmath). It compares the authored MathJSON expression and context before and after Symbio XML export, external decoding and encoding, and Symbio reimport. Expected exact matrix results and domain errors are checked in Symbio. The application test also compares the function names, ordered arguments and nesting against the original MathJSON. py-openmath provides the independent XML codec. Symbio performs the evaluation.

```sh
python3 -m venv /tmp/symbio-openmath-venv
/tmp/symbio-openmath-venv/bin/python -m pip install "setuptools<81" openmath==0.3.0
/tmp/symbio-openmath-venv/bin/python scripts/check-openmath-interop.py
```

## External CAS comparison

Run the same check with `--sympy` after installing `sympy==1.14.0` in the test environment. The test translates the OpenMath objects decoded by py-openmath into SymPy integers, rationals and matrices. Both CAS engines evaluate the same serialized expression. Forty-one successful linear algebra cases matched exactly, including the matrix product, inverse, exponent −2 scalar division by 3, rational trace, and rectangular, rank-deficient and zero-matrix ranks. For singularity and zero division, SymPy raised `NonInvertibleMatrixError` on the singular inverse and returned non-finite matrix entries on zero division. Symbio reported `domain_error` with no result in both cases. For indices beyond the last row or column, SymPy raised `IndexError` and Symbio reported `domain_error`.

```sh
/tmp/symbio-openmath-venv/bin/python -m pip install sympy==1.14.0
/tmp/symbio-openmath-venv/bin/python scripts/check-openmath-interop.py --sympy
```

The scalar product fixture compares an exact rational result of `7/2`. With different vector lengths, SymPy returns `ShapeError` and Symbio returns `domain_error`.

The cross product fixtures compare both operand orders with SymPy, using rational components. The two results have opposite signs in every component. A non-three-dimensional operand yields `domain_error` in Symbio.

The outer product fixtures compare a 2×3 rational result, its transpose, and the 3×2 result obtained by swapping operands with SymPy.

For `linalg6.matrix_tensor(A, B)`, Symbio uses the block convention in which block `(i, j)` is `A[i, j] * B`. The fixtures compare both operand orders and a three-factor product with SymPy.

The vector tensor fixtures compare both orders of a two-entry and a three-entry vector plus a three-factor product. The checks compare every exact component and the full result length.

Signed-value fixtures check a scalar matrix with diagonal entries of `-2/3` and exact division of a matrix containing a negative element. The comparisons check the authored form across the exchange and the resulting exact values.
