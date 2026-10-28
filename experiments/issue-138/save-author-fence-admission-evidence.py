#!/usr/bin/env python3
"""Preserve the early-admission regression and focused correction results."""
import hashlib
from pathlib import Path

root = Path(__file__).resolve().parent
parts = [
    'Author-fence wire admission correction, 2026-10-28 UTC.\n'
    'PR: https://github.com/link-foundation/link-cli/pull/139\n'
    'Baseline implementation aae1422fd88cf09d6a9414e3544b495cd5ca63ac.\n'
    'A record of 100 snow characters is 100 UTF-16 units but 300 UTF-8 bytes.\n'
    'JS/C# incorrectly reach JSON parsing despite a 256-byte pre-parse limit.\n'
    'JS parser-call interception and exact C# exception-type tests fail first.\n'
    'All ports now reject non-ASCII before parsing; valid canonical ASCII\n'
    'journals and published provider state remain unchanged.\n'
    'After: 7 Rust functions, 9 C# cases and 10 each Node/Bun/Deno pass.\n'
    'Node enforces 100% of this module\'s measured lines/branches/functions.\n'
    'Independent model: 6 complete interleavings, 24 shared wire fixtures.\n'
    'Focused runner: experiments/issue-138/run-author-fence-checks.sh.\n'
    'Strict Clippy, Rust/C# formatting, source size and 221 repository gates pass.\n'
    'Complete suites stay in CI per maintainer feedback. Fresh CI is pending.\n'
    'Display trims trailing whitespace; SHA256 identifies original raw logs.\n\n'
]
for name in ['js-before', 'csharp-before', 'model', 'after', 'bun', 'deno',
             'clippy', 'rust-format', 'csharp-format', 'repository', 'source-size', 'whitespace']:
    path = root / 'ci-logs' / f'author-fence-admission-{name}.log'
    data = path.read_bytes()
    parts.extend([f'--- {path.name} SHA256 {hashlib.sha256(data).hexdigest()} ---\n',
                  data.decode(), '\n'])
destination = root.parents[1] / 'dev/log/issues/138/pulls/139/author-fence-admission.txt'
destination.write_text('\n'.join(line.rstrip() for line in ''.join(parts).splitlines()).rstrip() + '\n')
