"""Private numeric-only protocol. Never parses/evaluates source strings as Python/SymPy code."""
import json
import re
import sys
try:
    import resource
    resource.setrlimit(resource.RLIMIT_CPU, (5, 5))
except (ImportError, ValueError, OSError):
    pass
try:
    import sympy as sp
except ImportError:
    sys.exit(42)
request = json.loads(sys.stdin.buffer.read(65537))
if request.get("protocol") != 1 or set(request) != {"protocol", "method", "coefficients"}:
    sys.exit(43)
def rational(s):
    if not isinstance(s, str) or len(s) > 160 or not re.fullmatch(r"-?[0-9]+(?:/[0-9]+)?", s):
        raise ValueError("rational literal")
    n, sep, d = s.partition("/")
    return sp.Rational(int(n), int(d) if sep else 1)
result = {"protocol": 1, "version": sp.__version__, "content": "1", "factors": []}
if request["method"] == "factor":
    coefficients = request["coefficients"]
    if not isinstance(coefficients, list) or not 1 <= len(coefficients) <= 33:
        sys.exit(43)
    x = sp.Symbol("_symbio_x")
    polynomial = sp.Poly.from_list([rational(c) for c in reversed(coefficients)], x, domain=sp.QQ)
    content, factors = polynomial.factor_list()
    result["content"] = str(content)
    result["factors"] = [{"coefficients": [str(c) for c in reversed(f.all_coeffs())], "exponent": int(n)} for f, n in factors]
elif request["method"] != "probe" or request["coefficients"]:
    sys.exit(43)
print(json.dumps(result, separators=(",", ":")))
