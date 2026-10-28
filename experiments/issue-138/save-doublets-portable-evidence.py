#!/usr/bin/env python3
"""Preserve reproducible heap-doublets regressions and official WASM counters."""
import hashlib
from pathlib import Path

root = Path(__file__).resolve().parent
repo = root.parents[1]
parts = [
    'Heap-doublets literal query regression, 2026-10-28 UTC.\n'
    'PR: https://github.com/link-foundation/link-cli/pull/139\n'
    'Immutable baseline: d3ba9bba0b3ad5577264235a1245c926f4afcd14\n'
    'Copy the final shared contract target into an archived baseline and run\n'
    'run-doublets-portable.sh BASELINE. Exactly three semantic contracts fail;\n'
    'eight behavior controls pass. The runner verifies cargo status 101.\n'
    'Its isolated target directory prevents stale fixed-library reuse. An\n'
    'earlier shared-target attempt unexpectedly passed and was rejected by\n'
    'the negative-control assertion; that attempt is not verification evidence.\n'
    'After the fix, all eleven native and twelve actual-WASM cases pass, plus\n'
    '24 existing native storage/decorator/transaction-observer cases. The\n'
    'existing ignored subprocess helper is executed by its parent lock test.\n'
    'The mapped regression reopens a real file, reuses its null pair, returns\n'
    'both duplicates and preserves every physical byte after read-only reuse.\n'
    'The independent 125-pattern oracle compares row sets, not traversal order.\n'
    'WASM also checks typed errors from unavailable filesystem constructors.\n'
    'No production coverage source, branch, region or instantiation is removed.\n'
    'Focused WASM counters are not complete-suite coverage. Unexecuted native\n'
    'filesystem paths and generic instantiations remain explicitly included.\n'
    'Queries use one usable endpoint index and filter all candidates; queries\n'
    'with only unindexed endpoints scan. Literal null/wildcard searches scan\n'
    'and stop at the first match without copying all rows. Ordinary nonzero\n'
    'pair searches retain their index. The public storage API is unchanged.\n'
    'Complete suites remain in CI per maintainer feedback. Fresh CI for this\n'
    'change and all wider unfinished R1-R12 criteria remain required.\n'
    'Related native command (same one-job/debug-zero/linker bounds as runner):\n'
    'cargo test --manifest-path rust/Cargo.toml --test doublets_storage_tests\n'
    '  --test doublets_decorators_tests --test transactions_doublets_observer_tests\n'
    '  -- --test-threads=1\n'
    'Focused lint command: cargo clippy --manifest-path rust/Cargo.toml\n'
    '  --test doublets_portable_tests --test doublets_storage_tests\n'
    '  --test doublets_decorators_tests --test transactions_doublets_observer_tests\n'
    '  -- -D warnings\n\n'
]
for relative in (
    'rust/src/storage/doublets_storage.rs', 'rust/tests/doublets_portable_tests.rs',
    'rust/tests/doublets_storage_tests.rs', 'rust/wasm/tests/storage.rs',
    'experiments/issue-138/run-doublets-portable.sh',
    'experiments/issue-138/run-doublets-portable-wasm-coverage.sh',
):
    raw = (repo / relative).read_bytes()
    parts.append(f'{relative}; SHA256 {hashlib.sha256(raw).hexdigest()}\n')


def retain(relative, predicate=None):
    path = root / relative
    raw = path.read_bytes()
    parts.append(f'\n--- {relative}; SHA256 {hashlib.sha256(raw).hexdigest()} ---\n')
    for number, line in enumerate(raw.decode().splitlines(), 1):
        if predicate is None or predicate(line):
            parts.append(f'{number}: {line}\n')


for name in (
    'doublets-portable-baseline-command', 'doublets-portable-after',
    'doublets-portable-existing-native', 'doublets-portable-clippy',
    'doublets-portable-wasm-baseline-summary', 'doublets-portable-wasm-summary',
    'doublets-portable-source-size', 'doublets-portable-shellcheck',
    'doublets-portable-tool-versions', 'doublets-portable-format',
):
    retain(f'ci-logs/{name}.log')
retain('doublets-portable-wasm-tests.log')
retain('ci-logs/doublets-portable-repository.log', lambda line:
       any(term in line for term in ('ℹ tests', 'ℹ pass', 'ℹ fail', 'ℹ cancelled')))
retain('ci-logs/doublets-portable-release.log', lambda line:
       line.startswith('Validating changeset') or line == 'Changeset validation passed')
destination = repo / 'dev/log/issues/138/pulls/139/doublets-portable.txt'
destination.write_text('\n'.join(line.rstrip() for line in ''.join(parts).splitlines()).rstrip() + '\n')
print(destination)
