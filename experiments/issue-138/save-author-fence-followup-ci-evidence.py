#!/usr/bin/env python3
"""Retain exact-head physical handoffs, strict platform gates and raw metrics."""
import hashlib
import itertools
import json
from pathlib import Path

root = Path(__file__).resolve().parent
head = '9ece2d163b0ad254c62093ad42be0b1ce579d872'
runs = json.loads(Path('/tmp/issue-138-recovery/runs-9ece2d1-fresh.json').read_text())
assert len(runs) == 9 and all(r['headSha'] == head for r in runs)
assert all(r['status'] == 'completed' and r['conclusion'] == 'success' for r in runs)
parts = [
    'Author fencing: exact-head follow-up CI, 2026-10-28 UTC.\n'
    'PR: https://github.com/link-foundation/link-cli/pull/139\n'
    f'Commit: {head}\n'
    'All nine workflows pass. This head verifies ASCII admission, physical\n'
    'handoffs and strict C# coverage. Later process exits, portable Rust\n'
    'contracts and native/WASM source coverage gates need fresh CI.\n\n'
]
for run in sorted(runs, key=lambda r: r['name']):
    parts.append(f"{run['name']}: {run['conclusion']}; created {run['createdAt']}; "
                 f"https://github.com/link-foundation/link-cli/actions/runs/{run['databaseId']}\n")


def evidence(name, predicate):
    path = root / 'ci-logs' / name
    with path.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    parts.append(f'\n{path.name}; SHA256 {digest}; one-based raw lines follow:\n')
    result, start = [], 0
    with path.open() as source:
        while chunk := list(itertools.islice(source, 1500)):
            for number, line in enumerate(chunk, start + 1):
                if predicate(line):
                    parts.append(f'{number}: {line.rstrip()}\n')
                    result.append(line)
            start += len(chunk)
    assert result, name
    return result


js = evidence('js-40704187896.log', lambda line:
              'PASS 50 fence complete-store handoffs' in line or
              'PASS real etcd author fencing:' in line or
              line.startswith('Unit tests and types\tUnit tests and complete production coverage\t') and
              ('ℹ tests 718' in line or 'engine-author-fence.js ' in line))
assert sum('PASS real etcd author fencing:' in line for line in js) == 2
assert sum('PASS 50 fence complete-store handoffs' in line for line in js) == 1
csharp = evidence('csharp-40704187916.log', lambda line:
                  line.startswith('Test (') and
                  ('\tRun tests\t' in line and ('Total tests: 2637' in line or 'Passed: 2637' in line) or
                   'AuthorFence.cs: 63/63 lines, 124/124 raw compiled branches' in line or
                   'PASS all author guard lines and raw branches' in line))
assert len(csharp) == 12
wasm = evidence('wasm-40704187898.log', lambda line:
                line.startswith('WASM coverage\tMeasure actual WebAssembly tests\t') and 'test result: ok.' in line)
assert len(wasm) == 9
assert sum(int(line.split('test result: ok. ')[1].split()[0]) for line in wasm) == 819
evidence('rust-40704188148.log', lambda line:
         line.startswith('Test (windows-2025)\tRun tests\t') and
         ('test a_stopped_server_executes_nothing ... ok' in line or
          'test explicit_takeover_fences_old_reads_writes_and_receipts ... ok' in line))
parts.append('\nArtifact downloads below preceded Windows completion; the final workflow states are verified above.\n')
for name in ['author-fence-platform-coverage-9ece2d1',
             'author-fence-native-artifact-9ece2d1', 'author-fence-native-coverage-9ece2d1',
             'author-fence-wasm-artifact-9ece2d1', 'author-fence-wasm-coverage-9ece2d1',
             'author-fence-followup-dependencies']:
    path = root / 'ci-logs' / f'{name}.log'
    raw = path.read_bytes()
    parts.extend([f'\n{path.name}; SHA256 {hashlib.sha256(raw).hexdigest()}\n', raw.decode()])
parts.append(
    '\nComplete native LCOV and actual-WASM official JSON still show uncovered\n'
    'production sources. The new component checker independently rejects\n'
    'these pre-audit reports; those rejections are expected evidence, not\n'
    'failures of this older workflow. No denominator is removed.\n'
    'Later local source-counter improvements do not complete whole-project\n'
    'coverage or wider R1-R12 criteria. PR139 remains draft and unmerged.\n')
destination = root.parents[1] / 'dev/log/issues/138/pulls/139/author-fence-followup-ci.txt'
destination.write_text('\n'.join(line.rstrip() for line in ''.join(parts).splitlines()).rstrip() + '\n')
