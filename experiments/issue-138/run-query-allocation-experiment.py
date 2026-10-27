#!/usr/bin/env python3
"""Reuse a compiled library; run 15 finite queries under memory/stack limits."""
from pathlib import Path
import resource
import subprocess

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]
library = max((ROOT / 'rust/target/debug/deps').glob('liblink_cli-*.rlib'),
              key=lambda path: path.stat().st_mtime)
compiler = ['rustc', '--edition=2021', str(BASE / 'query-allocation-matrix.rs'),
            '-L', f'dependency={ROOT / "rust/target/debug/deps"}',
            '--extern', f'link_cli={library}', '-o', str(BASE / 'query-allocation-matrix')]
with (BASE / 'query-allocation-matrix-compile.log').open('w') as log:
    print('Library:', library, file=log, flush=True)
    result = subprocess.run(compiler, stdout=log, stderr=subprocess.STDOUT, check=False)
assert result.returncode == 0, 'See query-allocation-matrix-compile.log'


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024,) * 2)
    resource.setrlimit(resource.RLIMIT_STACK, (8 * 1024 * 1024,) * 2)


with (BASE / 'query-allocation-matrix-before.log').open('w') as log:
    result = subprocess.run([str(BASE / 'query-allocation-matrix')], stdout=log,
                            stderr=subprocess.STDOUT, preexec_fn=limits, check=False)
assert result.returncode == 0, 'See query-allocation-matrix-before.log'
print('Completed finite allocation comparison; inspect query-allocation-matrix-before.log.')
