#!/usr/bin/env python3
"""Retain native coverage controls without replacing official denominators."""
import hashlib
from pathlib import Path

root = Path(__file__).resolve().parent
parts = [
    'Rust author-guard coverage audit, 2026-10-28 UTC.\n'
    'PR: https://github.com/link-foundation/link-cli/pull/139\n'
    'Production is unchanged from the ASCII admission correction a711106.\n'
    'The independently digest-verified aae1422 native artifact predates that\n'
    'correction and rejects at 216/217 LCOV lines and 62/68 branches. These\n'
    'export counts do not replace official LLVM JSON file summaries.\n'
    'Six additional portable public-contract tests cover finite byte budgets,\n'
    'unknown tags, retained predecessors, absent/forged ownership, independent\n'
    'group lengths, provider errors and invalid guard arguments. The shared\n'
    'model now supplies 25 fixtures. All 13 native contract tests, 15 C# cases\n'
    'and 10 tests on each of Node/Bun/Deno pass in focused local execution.\n'
    'The first focused LLVM measurement still rejects three uncovered error\n'
    'propagation regions. The final same-provider controls cover those outcomes.\n'
    'After cleaning prior workspace coverage artifacts, the official native\n'
    'file summary is 217/217 lines, 70/70 branches, 22/22 functions and\n'
    '313/313 regions. Instantiations remain 22/38, explicitly unfinished.\n'
    'The gate retains and prints that counter; it requires complete source\n'
    'lines/branches/functions/regions without claiming full instantiation or\n'
    'whole-project coverage. Full native/WASM CI now runs it after measurement.\n'
    'CI keeps full LCOV plus official LLVM per-file/global JSON summaries,\n'
    'including reports when the new gate fails. No source, branch, region or\n'
    'instantiation is excluded and no production code is restructured.\n'
    'Full suites remain in CI per maintainer feedback; fresh CI is pending.\n'
    'Display trims trailing whitespace; hashes identify original raw logs.\n\n'
]
names = [
    'author-fence-rust-artifact-before', 'author-fence-rust-coverage-tool',
    'author-fence-rust-coverage-draft', 'author-fence-rust-coverage-clean',
    'author-fence-rust-coverage-tests-complete', 'author-fence-rust-coverage-after',
    'author-fence-rust-coverage-model', 'author-fence-rust-coverage-focused',
    'author-fence-rust-coverage-bun', 'author-fence-rust-coverage-deno',
    'author-fence-rust-coverage-clippy', 'author-fence-rust-coverage-repository',
    'author-fence-rust-coverage-source-size', 'author-fence-rust-coverage-format',
    'author-fence-rust-coverage-workflows', 'author-fence-rust-coverage-shellcheck',
    'author-fence-rust-coverage-whitespace',
]
for name in names:
    path = root / 'ci-logs' / f'{name}.log'
    raw = path.read_bytes()
    parts.extend([f'--- {path.name}; SHA256 {hashlib.sha256(raw).hexdigest()} ---\n',
                  raw.decode(), '\n'])
destination = root.parents[1] / 'dev/log/issues/138/pulls/139/author-fence-rust-coverage.txt'
destination.write_text('\n'.join(line.rstrip() for line in ''.join(parts).splitlines()).rstrip() + '\n')
