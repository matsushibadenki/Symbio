#!/usr/bin/env python3
"""Independent symbolic derivative checks of real CLI-generated SEG primitives.
Run with an existing Python containing SymPy; no packages are installed.
"""
import functools
import json
from pathlib import Path
import subprocess
import tempfile
import sympy as sp

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT/'target/debug/symbio-cli'
subprocess.run(['cargo', 'build', '--locked', '--offline', '-p', 'symbio-cli'], cwd=ROOT, check=True)
x = sp.Symbol('x', real=True)

def decode(nodes):
    @functools.lru_cache(None)
    def node(i):
        n = nodes[i]; op = n['op']; a = n['args']
        if op == 'number':
            numerator, sep, denominator = a.partition('/')
            return sp.Rational(int(numerator), int(denominator) if sep else 1)
        if op == 'symbol':
            assert a == 'x', a
            return x
        if op == 'call':
            return {'sin': sp.sin, 'cos': sp.cos, 'exp': sp.exp}[a[0]](node(a[1]))
        left, right = (node(k) for k in a)
        return {'add': lambda: left+right, 'sub': lambda: left-right,
                'mul': lambda: left*right, 'div': lambda: left/right,
                'pow': lambda: left**right}[op]()
    return node

count = 0
with tempfile.TemporaryDirectory(prefix='symbio-independent-integrals-') as directory:
    folder = Path(directory)
    for function, symbolic in [('Sin', sp.sin), ('Cos', sp.cos), ('Exp', sp.exp)]:
        for slope in [sp.Rational(-3), sp.Rational(1, 2)]:
            for degree in range(4):
                polynomial = ['Add', ['Power', 'x', degree], 1] if degree else 2
                affine = ['Add', ['Multiply', ['Rational', int(slope.p), int(slope.q)], 'x'], ['Rational', 1, 3]]
                expression = ['Integrate', ['Multiply', polynomial, [function, affine]], 'x']
                source = folder/f'{count}.json'; project = folder/f'{count}.symbio'
                source.write_text(json.dumps(expression))
                result = subprocess.run([CLI, 'import-mathjson', source, project], capture_output=True, text=True, check=True)
                cell = json.loads(result.stdout)['cells'][0]
                assert cell['output'] is not None, (expression, cell.get('calculation_error'))
                assert cell['output']['trust'] == 'symbolic' and cell['output']['precision'] == 'exact'
                data = json.loads(project.read_text()); node = decode(data['graph']['nodes'])
                stored = data['branches'][data['active']]['cells'][0]
                primitive = node(stored['output']['root'])
                expected = (x**degree+1)*symbolic(slope*x+sp.Rational(1, 3))
                residual = sp.simplify(sp.diff(primitive, x)-expected)
                assert residual == 0, (expression, primitive, residual)
                count += 1
print(f'PASS: {count} CLI primitives independently differentiated in SymPy {sp.__version__}; exact symbolic residuals are zero')
