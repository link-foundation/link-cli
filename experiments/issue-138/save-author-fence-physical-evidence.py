#!/usr/bin/env python3
"""Retain focused physical handoff results without claiming pending full CI."""
import hashlib
from pathlib import Path

root = Path(__file__).resolve().parent
parts = [
    'Physical author-fence handoffs, 2026-10-28 UTC.\n'
    'PR: https://github.com/link-foundation/link-cli/pull/139\n'
    'Base aae1422fd88cf09d6a9414e3544b495cd5ca63ac.\n'
    'The first baseline fails because its optional Rust recipe is absent.\n'
    'It is a missing selected API, not an existing-feature regression.\n'
    'After: two real plain/mirrored Rust -> C# -> Node -> Rust/C# handoffs pass.\n'
    'Complete active claims, retained retries, refused superseded reads/writes/\n'
    'retries and two-key publication survive reopening. Read-only operations\n'
    'preserve every file hash; ordinary data CAS preserves fence generation.\n'
    'Run: python3 experiments/issue-138/author-fence-native.py.\n'
    'The script bounds native stack/address space and JS/.NET heaps.\n'
    'Rust builds use one job, no debug info/incremental and one linker thread.\n'
    'C# Release builds use one MSBuild worker, CPU count two and a 1 GiB GC cap.\n'
    'Clippy denies warnings with every optional native_lsm recipe selected.\n'
    'The fifty-handoff five-runtime matrix is wired to CI; its new execution\n'
    'is pending. Complete suites remain in CI per maintainer feedback.\n'
    'Clean-close controls do not certify hardware power loss or bypass safety.\n'
    'Display trims trailing whitespace; digests identify original raw logs.\n\n'
]
for name in ['before', 'build', 'csharp-build', 'after', 'clippy', 'format-verified',
             'repository', 'source-size', 'release-tests', 'whitespace']:
    path = root / 'ci-logs' / f'author-fence-native-{name}.log'
    data = path.read_bytes()
    parts.extend([f'--- {path.name} SHA256 {hashlib.sha256(data).hexdigest()} ---\n',
                  data.decode(), '\n'])
destination = root.parents[1] / 'dev/log/issues/138/pulls/139/author-fence-physical.txt'
destination.write_text('\n'.join(line.rstrip() for line in ''.join(parts).splitlines()).rstrip() + '\n')
