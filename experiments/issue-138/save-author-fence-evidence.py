#!/usr/bin/env python3
"""Preserve bounded local fencing results and original raw-log digests."""
import hashlib
from pathlib import Path
root = Path(__file__).resolve().parent
names = ['before', 'rust-before', 'csharp-before', 'model', 'js', 'bun', 'deno',
         'rust', 'csharp', 'clippy', 'types', 'rust-format', 'format-verified', 'csharp-example', 'gates']
parts = ['Selected author fencing; base c14beca86e01bafb8ca9ceda9bffd5820e0d90d9.\n'
         'PR: https://github.com/link-foundation/link-cli/pull/139\n'
         'Focused local checks, 2026-10-28 UTC; complete suites remain in CI.\n'
         'Before logs demonstrate absent selected APIs, not existing-feature regressions.\n'
         'Final results: seven Rust functions; eight C# cases; nine each Node/Bun/Deno.\n'
         'Node enforces 100% lines, branches and functions of the new production module.\n'
         'Independent model: six complete read/CAS schedules, precheck-only negative\n'
         'control and 22 canonical/admission fixtures.\n'
         'Commands/limits: experiments/issue-138/run-author-fence-checks.sh.\n'
         'Clippy additionally builds the selected etcd CLI with warnings denied.\n'
         'The real-etcd/actual-WASM composition recipe is wired to CI, not yet executed.\n'
         'Display trims trailing whitespace; SHA256 identifies original raw logs.\n\n']
for name in names:
    path = root / 'ci-logs' / f'author-fence-{name}.log'
    data = path.read_bytes()
    parts += [f'--- {path.name} SHA256 {hashlib.sha256(data).hexdigest()} ---\n', data.decode(), '\n']
for name in ['repository', 'source-size', 'changeset', 'release-tests', 'shellcheck', 'whitespace']:
    path = root / f'author-fence-{name}.log'
    data = path.read_bytes()
    parts += [f'--- {path.name} SHA256 {hashlib.sha256(data).hexdigest()} ---\n', data.decode(), '\n']
destination = root.parents[1] / 'dev/log/issues/138/pulls/139/author-fencing.txt'
destination.write_text('\n'.join(line.rstrip() for line in ''.join(parts).splitlines()).rstrip() + '\n')
