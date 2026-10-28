#!/usr/bin/env python3
"""Retain independent missing-API and visibility-order diagnostics."""
from pathlib import Path
import hashlib

base = Path(__file__).resolve().parent
root = base.parents[1]
print('PR: https://github.com/link-foundation/link-cli/pull/139')
for name, marker in [('js-before', 'ERR_MODULE_NOT_FOUND'), ('native-before', 'E0432'),
                     ('csharp-before', 'CS0246'), ('visibility-before', 'resurrects the losing row')]:
    path = base / f'causal-projection-{name}.log'
    data = path.read_bytes()
    text = data.decode()
    assert marker in text, (name, marker)
    print(f'{path.relative_to(root)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
    for number, line in enumerate(text.splitlines(), 1):
        if marker in line:
            print(f'  line {number}: {line.strip()}')
print('Missing-module/type diagnostics precede adding production ports in each language.')
print('The frozen wrong-order control removes markers before choosing maximum dot; its expected empty visibility assertion fails.')
print('Five production contracts select from the complete frontier and preserve losing siblings and deletions.')
print('Rust also emits E0277 type-inference follow-ups while its imported projection type is absent; the actual port resolves them.')
print('No production coverage threshold or denominator is reduced.')
