"""Optional offline SymPy oracle for the implemented subset, never a runtime dependency."""
from fractions import Fraction
from pathlib import Path
import json
import subprocess
import tempfile
import sympy as sp

root = Path(__file__).resolve().parents[1]
subprocess.run(["cargo", "build", "--locked", "--offline", "-p", "symbio-cli"], cwd=root, check=True)
cli = root / "target/debug/symbio-cli"
x, y, z = sp.symbols("x y z", real=True)
complex_x = sp.Symbol("x", complex=True)
cases = [
    ("partial equation", "z=x^2-y^2", "substitute", {"x": "2"}, sp.Eq(z, 4-y**2), None),
    ("rational decimal", "x^2+y", "substitute", {"x": "0.1", "y": "1/3"}, sp.Rational(103, 300), None),
    ("real square root", r"\sqrt{x^2}", "compute", {}, sp.Abs(x), None),
    ("complex square root", r"\sqrt{x^2}", "compute", {}, sp.sqrt(complex_x**2), "complex"),
    ("partial derivative", "x^2-y^2", "differentiate", {}, -2*y, None),
    ("definite integral", r"\int_{0}^{1} x^2\,dx", "compute", {}, sp.Rational(1, 3), None),
]

def decode(nodes, index, complex_domain=False):
    node = nodes[index]
    op, args = node["op"], node.get("args")
    def child(i): return decode(nodes, i, complex_domain)
    if op == "number":
        n = Fraction(args)
        return sp.Rational(n.numerator, n.denominator)
    if op == "symbol":
        return complex_x if args == "x" and complex_domain else sp.Symbol(args, real=True)
    if op in ("add", "sub", "mul", "div", "pow"):
        a, b = map(child, args)
        return {"add": lambda: a+b, "sub": lambda: a-b, "mul": lambda: a*b,
                "div": lambda: a/b, "pow": lambda: a**b}[op]()
    if op == "call":
        return {"sqrt": sp.sqrt, "abs": sp.Abs, "sin": sp.sin, "cos": sp.cos,
                "exp": sp.exp, "ln": sp.log, "log": sp.log}[args[0]](child(args[1]))
    if op == "equation": return sp.Eq(child(args[0]), child(args[1]), evaluate=False)
    raise AssertionError(f"Unexpected oracle node: {op}")

for label, source, operation, values, expected, domain in cases:
    with tempfile.TemporaryDirectory(prefix="symbio-oracle-") as directory:
        path = str(Path(directory) / "study.symbio")
        subprocess.run([cli, "init", path], check=True, capture_output=True)
        revision = 0
        commands = []
        if domain:
            commands.append({"method": "math.assume", "symbol": "x", "assumption": {"domain": domain}})
        commands.append({"method": "math.define_latex", "id": 1, "name": "f", "source": source})
        commands.append({"method": "math.substitute", "id": 1, "values": values} if operation == "substitute"
                        else {"method": "math.transform", "id": 1, "operation": operation,
                              "variable": "y" if operation == "differentiate" else "x", "bindings": {}})
        requests = []
        for command in commands:
            requests.append({"expected_revision": revision, "actor": "human", "command": command})
            revision += 1
        requests.append({"method": "math.export"})
        response = subprocess.run([cli, "serve", path], input="\n".join(map(json.dumps, requests))+"\n",
                                  text=True, capture_output=True, check=True)
        rows = [json.loads(line) for line in response.stdout.splitlines()]
        assert all(row["ok"] for row in rows), (label, rows)
        project = rows[-1]["result"]
        cell = next(c for c in project["branches"]["main"]["cells"] if c["id"] == 1)
        actual = decode(project["graph"]["nodes"], cell["output"]["root"], domain == "complex")
        if isinstance(expected, sp.Equality):
            assert isinstance(actual, sp.Equality) and actual.lhs == expected.lhs
            assert sp.simplify(actual.rhs-expected.rhs) == 0, (label, actual, expected)
        else:
            assert sp.simplify(actual-expected) == 0, (label, actual, expected)
        assert cell["output"]["trust"] == "symbolic" and cell["output"]["precision"] == "exact"
        print(f"PASS: {label}")
print(f"PASS: {len(cases)} bounded differential cases against SymPy {sp.__version__}; no general CAS conformance claim")
