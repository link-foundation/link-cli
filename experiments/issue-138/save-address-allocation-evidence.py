#!/usr/bin/env python3
"""Preserve the small local allocator results with their original file digests."""
from pathlib import Path
import hashlib

names = [
    'address-allocation-before.log', 'address-allocation-model.log',
    'address-allocation-focused.log', 'address-allocation-response-before.log',
    'address-allocation-js.log', 'address-allocation-bun.log', 'address-allocation-deno.log',
    'address-allocation-types.log', 'address-allocation-native.log',
    'address-allocation-exits.log', 'address-allocation-clippy.log',
    'address-allocation-format-verified.log', 'address-allocation-gates.log',
]
root = Path(__file__).resolve().parent / 'ci-logs'
parts = ['Selected permanent address allocations; base 5cc37328586f36fb4ea6adb7d842cfc8412e1681.\n'
         'Local focused checks, 2026-10-28 UTC. Complete suites remain in CI.\n'
         'Display trims trailing whitespace; digests identify original raw logs.\n'
         'Commands and limits: experiments/issue-138/run-address-allocation-checks.sh;\n'
         'experiments/issue-138/address-allocation-native.py; node-only run-causal-faults.py.\n'
         'JS missing-module baseline records the absent new API, not an allocator bug.\n'
         'Combined first checks contain seven JS cases; following JS logs add the\n'
         'reproduced malformed-reply regression and a canonical-terminator control;\n'
         'the final nine cases finish at 100% coverage.\n\n']
for name in names:
    data = (root / name).read_bytes()
    parts.extend([f'--- {name} SHA256 {hashlib.sha256(data).hexdigest()} ---\n', data.decode(), '\n'])
parts.append('Repository gate initially rejected the local root ci-logs directory (220/221 passed).\n'
             'Logs moved to experiments/issue-138/ci-logs; rerun passes 221/221, no policy changes.\n'
             'Rust run 40684594422 on cbddc13 was superseded after 5cc3732 triggered its successor.\n'
             'Its raw log is experiments/issue-138/ci-logs/rust-40684594422.log.\n'
             'Line 35392: Windows Run tests, 2026-10-28T12:37:34.9302211Z: The operation was canceled.\n'
             'Other completed Rust jobs passed; this cancellation is not a passing Windows result.\n')
(root.parents[2] / 'dev/log/issues/138/pulls/139/address-allocation.txt').write_text(
    '\n'.join(line.rstrip() for line in ''.join(parts).splitlines()) + '\n')
