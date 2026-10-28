#!/usr/bin/env python3
"""Retain independent author-guard admission controls and complete denominators."""
import hashlib
from pathlib import Path

root = Path(__file__).resolve().parent
parts = [
    'Author-fence C# contract coverage, 2026-10-28 UTC.\n'
    'PR: https://github.com/link-foundation/link-cli/pull/139\n'
    'Production baseline: a711106; no production changes in this step.\n'
    'Before: nine focused cases pass but the independent raw gate rejects\n'
    '62/63 source lines and 106/124 compiled branches in Release.\n'
    'Six additional public-contract cases cover malformed providers, finite\n'
    'budgets, capabilities, retained request identity, contention, absent\n'
    'ownership and caller-owned arguments. Debug additionally exposes the\n'
    'missing unknown-record-tag outcome; that provider frame is now tested.\n'
    'After: all 15 focused cases pass in Debug and Release. All production\n'
    'sources remain instrumented. The strict component gate checks every raw\n'
    'branch, including generated methods with no source-line map. Complete\n'
    'focused-report denominators remain below and are not full-suite coverage.\n'
    'CI now checks this gate against each complete platform test report.\n'
    'No production source, branch, exclusion or instrumentation is removed.\n'
    'Full suites remain in CI per maintainer feedback. Fresh CI is pending.\n'
    'Display trims trailing whitespace; SHA256 identifies original raw logs.\n\n'
]
names = [
    'author-fence-focused-coverage-before', 'author-fence-csharp-coverage-before',
    'author-fence-csharp-coverage-debug',
    'author-fence-focused-coverage-Debug-complete', 'author-fence-csharp-coverage-Debug-complete',
    'author-fence-focused-coverage-Release-complete', 'author-fence-csharp-coverage-Release-complete',
    'author-fence-coverage-format', 'author-fence-coverage-repository',
    'author-fence-coverage-source-size', 'author-fence-coverage-whitespace',
]
for name in names:
    path = root / 'ci-logs' / f'{name}.log'
    data = path.read_bytes()
    parts.extend([f'--- {path.name} SHA256 {hashlib.sha256(data).hexdigest()} ---\n',
                  data.decode(), '\n'])
destination = root.parents[1] / 'dev/log/issues/138/pulls/139/author-fence-coverage.txt'
destination.write_text('\n'.join(line.rstrip() for line in ''.join(parts).splitlines()).rstrip() + '\n')
