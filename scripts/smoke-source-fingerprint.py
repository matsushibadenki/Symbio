#!/usr/bin/env python3
"""Exercise the real compiled build script against an isolated source tree."""
from pathlib import Path
import os
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
subprocess.run(['cargo', 'build', '--locked', '--offline', '-p', 'symbio-core'], cwd=root, check=True)
script = max((root/'target/debug/build').glob('symbio-core-*/build-script-build'), key=lambda p: p.stat().st_mtime)
env = dict(os.environ, RUSTC=shutil.which('rustc'), TARGET='test-target', OPT_LEVEL='0', CARGO_CFG_TARGET_FEATURE='')
with tempfile.TemporaryDirectory(prefix='symbio-source-manifest-') as d:
    folder = Path(d)
    (folder/'src').mkdir()
    for name, data in [('Cargo.toml', '[package]'), ('build.rs', '// build'), ('src/lib.rs', '// core'), ('src/cas_worker.py', '# worker v1')]:
        (folder/name).write_text(data)
    def fingerprint():
        result = subprocess.run([script], cwd=folder, env=env, capture_output=True, text=True, check=True)
        return next(line for line in result.stdout.splitlines() if line.startswith('cargo:rustc-env=SYMBIO_CORE_SOURCE_SHA256='))
    first = fingerprint()
    assert fingerprint() == first
    (folder/'src/cas_worker.py').write_text('# worker v2')
    second = fingerprint()
    assert second != first
    (folder/'src/cas_worker.py').unlink()
    assert fingerprint() not in (first, second)
print('PASS: real build script is deterministic and detects worker-only edits/deletion')
