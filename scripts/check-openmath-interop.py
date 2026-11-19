#!/usr/bin/env python3
"""Roundtrip Symbio CLI documents through the independent py-openmath XML codec."""
import argparse
import json
from importlib.metadata import version
from pathlib import Path
import subprocess
import tempfile

from openmath import decoder, encoder, openmath as om


ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "target/debug/symbio-cli"


def run(*args):
    return subprocess.check_output([CLI, *args], text=True)


def matrix(*rows):
    return ["List", *[["List", *row] for row in rows]]


def evaluate_matrix_xml(document, cas):
    """Translate only the operators exercised by the generated fixtures."""
    def evaluate(node):
        if isinstance(node, om.OMInteger):
            return cas.Integer(node.integer)
        if not isinstance(node, om.OMApplication) or not isinstance(node.elem, om.OMSymbol):
            raise ValueError(f"Unsupported OpenMath object: {node!r}")
        head = (node.elem.cd, node.elem.name)
        args = [evaluate(arg) for arg in node.arguments]
        if head == ("linalg2", "matrixrow"):
            return args
        if head == ("linalg2", "matrix"):
            return cas.Matrix(args)
        if head == ("symbio_linalg1", "trace"):
            assert node.elem.cdbase == "urn:symbio:cd"
            return cas.trace(*args)
        operations = {
            ("arith1", "plus"): lambda a, b: a + b,
            ("arith1", "minus"): lambda a, b: a - b,
            ("arith1", "times"): lambda a, b: a * b,
            ("arith1", "divide"): lambda a, b: a / b,
            ("arith1", "power"): lambda a, b: a ** b,
            ("linalg1", "transpose"): lambda a: a.T,
            ("linalg1", "determinant"): lambda a: a.det(),
            ("linalg4", "rank"): lambda a: a.rank(),
            ("nums1", "rational"): cas.Rational,
        }
        return operations[head](*args)

    # Fixtures have empty assumptions in the Symbio context envelope.
    value = evaluate(document.omel.obj)
    if isinstance(value, cas.MatrixBase):
        if any(entry.is_finite is not True for entry in value):
            return None, "non-finite entries"
        entries = ", ".join(map(str, value))
        return f"matrix({value.rows}x{value.cols}; {entries})", None
    return str(value), None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sympy", action="store_true", help="also evaluate matrix cases in SymPy")
    args = parser.parse_args()
    if args.sympy:
        import sympy
        from sympy.matrices.exceptions import NonInvertibleMatrixError

    subprocess.run(
        ["cargo", "build", "--locked", "--offline", "-p", "symbio-cli"],
        cwd=ROOT,
        check=True,
    )
    a = matrix([2, 1], [0, 3])
    b = matrix([1, 0], [2, 1])
    row = matrix([1, 2, 3])
    singular = matrix([1, 2], [2, 4])
    function = ["Apply", {"sym": "outer_2"}, "y",
                ["Apply", {"sym": "inner"}, {"num": "2"}, "x"],
                ["Apply", {"sym": "constant_fn"}]]
    cases = [
        ("transpose", ["Transpose", row], "matrix(3x1; 1, 2, 3)", None),
        ("trace", ["Trace", matrix([["Rational", 1, 2], 7], [3, ["Rational", 1, 3]])], "5/6", None),
        ("rank-rectangular", ["Rank", matrix([0, 0, 1], [0, ["Rational", 1, 2], 3])], "2", None),
        ("rank-deficient", ["Rank", singular], "1", None),
        ("rank-zero", ["Rank", matrix([0, 0, 0])], "0", None),
        ("determinant", ["Determinant", a], "6", None),
        ("sum", ["Add", a, b], "matrix(2x2; 3, 1, 2, 4)", None),
        ("difference", ["Subtract", a, b], "matrix(2x2; 1, 1, -2, 2)", None),
        ("product", ["MatrixMultiply", a, b], "matrix(2x2; 4, 1, 6, 3)", None),
        ("scalar-product", ["Multiply", 2, a], "matrix(2x2; 4, 2, 0, 6)", None),
        ("inverse", ["Inverse", a], "matrix(2x2; 1/2, -1/6, 0, 1/3)", None),
        ("power", ["Power", a, -2], "matrix(2x2; 1/4, -5/36, 0, 1/9)", None),
        ("division", ["Divide", a, 3], "matrix(2x2; 2/3, 1/3, 0, 1)", None),
        ("zero-division", ["Divide", a, 0], None, "domain_error"),
        ("singular", ["Inverse", singular], None, "domain_error"),
        ("application", function, "outer_2(y, inner(2, x), constant_fn())", None),
    ]

    with tempfile.TemporaryDirectory(prefix="symbio-openmath-interop-") as directory:
        for name, expression, result, error in cases:
            case = Path(directory) / name
            case.mkdir()
            source = case / "source.json"
            source.write_text(json.dumps(expression), encoding="utf-8")
            original = case / "original.symbio"
            run("import-mathjson", source, original)
            before = case / "before.json"
            run("export-mathjson", original, before)
            xml = case / "symbio.xml"
            run("export-openmath", original, xml)

            external = case / "external.xml"
            document = decoder.decode_bytes(xml.read_bytes())
            external.write_bytes(encoder.encode_bytes(document))

            reloaded = case / "reloaded.symbio"
            snapshot = json.loads(run("import-openmath", external, reloaded))
            after = case / "after.json"
            run("export-mathjson", reloaded, after)
            restored = json.loads(after.read_text(encoding="utf-8"))
            assert restored == json.loads(before.read_text(encoding="utf-8")), name
            if name == "application":
                assert restored["expression"] == function
            if name == "trace" or name.startswith("rank-"):
                assert restored["expression"][0] == expression[0], name
            cell = snapshot["cells"][0]
            assert cell["result_text"] == result, (name, cell["result_text"])
            assert cell.get("calculation_error") == error, name
            if args.sympy and name != "application":
                try:
                    cas_result, cas_error = evaluate_matrix_xml(
                        decoder.decode_bytes(external.read_bytes()), sympy
                    )
                except (NonInvertibleMatrixError, ZeroDivisionError) as exc:
                    cas_result, cas_error = None, type(exc).__name__
                assert cas_result == cell["result_text"], (name, cas_result, cell)
                if name == "singular":
                    assert cas_error == "NonInvertibleMatrixError", cas_error
                elif name == "zero-division":
                    assert cas_error in {"non-finite entries", "ZeroDivisionError"}, cas_error
                else:
                    assert cas_error is None, (name, cas_error)
                if cas_error:
                    print(f"PASS {name}: SymPy {cas_error}, Symbio {error}")
                    continue
            print(f"PASS {name}")

    print(f"PASS {len(cases)} cases through py-openmath {version('openmath')}")
    if args.sympy:
        print(f"PASS {len(cases) - 1} matrix cases against SymPy {sympy.__version__}")


if __name__ == "__main__":
    main()
