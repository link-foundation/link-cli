#!/usr/bin/env python3
"""Retain finite application-exit and exact takeover recovery controls."""
import hashlib
from pathlib import Path

root = Path(__file__).resolve().parent
parts = [
    'Author-fence process recovery, 2026-10-28 UTC.\n'
    'PR: https://github.com/link-foundation/link-cli/pull/139\n'
    'Baseline 9ece2d1 has clean-close recipes but no selected process actions.\n'
    'The new recipe first fails on the unsupported fence process layout.\n'
    'No production code or protocol bytes change in this step.\n'
    'After: 12 focused reaped exits pass, four each in Node, Rust and C#.\n'
    'Each runtime checks plain/mirrored stores before/after takeover CAS,\n'
    'retained original predecessor/token, superseded guards and a third owner.\n'
    'Local process controls reopen with the same runtime; cross-language\n'
    'clean-close controls additionally pass two focused physical handoffs.\n'
    'The full five-runtime process matrix stays in CI: 20 exits, 100 checks.\n'
    'All ordinary blocked opens preserve crash evidence. Explicit recovery\n'
    'quarantines writer markers only after the child has exited and is reaped.\n'
    'A committed takeover preserves the previous data until a guarded write.\n'
    'Retained durable retry and read-only reopening preserve every file hash.\n'
    'Inputs are finite; subprocesses have bounded stacks, Rust address space,\n'
    'JS heap and .NET heap/CPU budgets. All children are reaped.\n'
    'These controls do not establish arbitrary torn-write or hardware\n'
    'power-loss safety, automatic recovery authority or bypass admission.\n'
    'Complete suites stay in CI. Fresh exact-head CI remains pending.\n'
    'Display trims trailing whitespace; SHA256 identifies original raw logs.\n\n'
]
for name in ['before', 'node', 'rust-build', 'rust', 'csharp-build', 'csharp',
             'clean-handoffs', 'clippy', 'csharp-format', 'rust-format',
             'shellcheck', 'repository', 'source-size', 'whitespace']:
    path = root / 'ci-logs' / f'author-fence-process-{name}.log'
    data = path.read_bytes()
    parts.extend([f'--- {path.name} SHA256 {hashlib.sha256(data).hexdigest()} ---\n', data.decode(), '\n'])
destination = root.parents[1] / 'dev/log/issues/138/pulls/139/author-fence-process.txt'
destination.write_text('\n'.join(line.rstrip() for line in ''.join(parts).splitlines()).rstrip() + '\n')
