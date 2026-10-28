#!/usr/bin/env python3
"""Preserve immutable initial fencing CI outcomes and selected raw log lines."""
import hashlib
import itertools
import json
from pathlib import Path

root = Path(__file__).resolve().parent
recovery = Path('/tmp/issue-138-recovery')
head = 'aae1422fd88cf09d6a9414e3544b495cd5ca63ac'
runs = [r for r in json.loads((recovery / 'runs-aae1422-fresh.json').read_text()) if r['headSha'] == head]
assert len(runs) == 9 and all(r['status'] == 'completed' for r in runs)
assert [(r['name'], r['conclusion']) for r in runs if r['conclusion'] != 'success'] == [('Dependencies', 'failure')]
parts = [
    'Initial author fencing: exact-head CI evidence, 2026-10-28 UTC.\n'
    'PR: https://github.com/link-foundation/link-cli/pull/139\n'
    f'Commit: {head}\n'
    'Eight workflows pass; Dependencies fails on a newly published action.\n'
    'This head precedes physical fence handoffs, ASCII admission and additional\n'
    'C# contracts. Those later changes require their own exact-head CI.\n\n'
]
for run in sorted(runs, key=lambda r: r['name']):
    parts.append(f"{run['name']}: {run['conclusion']}; created {run['createdAt']}; "
                 f"https://github.com/link-foundation/link-cli/actions/runs/{run['databaseId']}\n")
parts.append('\nRaw logs are retained locally. SHA256 and one-based selected line references follow.\n')

def evidence(name, predicate):
    path = root / 'ci-logs' / name
    with path.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    parts.append(f'\n{path.relative_to(root.parents[1])}: SHA256 {digest}\n')
    result = []
    with path.open() as source:
        start = 0
        while chunk := list(itertools.islice(source, 1500)):
            for number, line in enumerate(chunk, start + 1):
                if predicate(line):
                    parts.append(f'{number}: {line.rstrip()}\n')
                    result.append(line)
            start += len(chunk)
    assert result, name
    return result

js = evidence('js-40698279214.log', lambda line:
              'PASS real etcd author fencing:' in line or
              line.startswith('Unit tests and types\tUnit tests and complete production coverage\t') and
              ('ℹ tests 717' in line or 'engine-author-fence.js ' in line))
assert sum('PASS real etcd author fencing:' in line for line in js) == 2
csharp = evidence('csharp-40698279522.log', lambda line:
                  line.startswith('Test (') and '\tRun tests\t' in line and
                  ('Total tests: 2630' in line or 'Passed: 2630' in line))
assert len(csharp) == 6
wasm = evidence('wasm-40698279543.log', lambda line:
                line.startswith('WASM coverage\tMeasure actual WebAssembly tests\t') and 'test result: ok.' in line)
assert len(wasm) == 9
assert sum(int(line.split('test result: ok. ')[1].split()[0]) for line in wasm) == 819
evidence('rust-40698279315.log', lambda line:
         line.startswith('Test (windows-2025)\tRun tests\t') and 'test a_stopped_server_executes_nothing ... ok' in line)
evidence('dependencies-40698279431.log', lambda line: 'install-action' in line and ('v2.87.33' in line or 'v2.88.0' in line))
platform = root / 'ci-logs/author-fence-platform-coverage-aae1422.log'
parts.extend(['\nComplete C# artifacts, independently downloaded and digest verified:\n', platform.read_text()])
parts.append(
    '\nActual-WASM total: 819 tests in nine measured binaries. JavaScript: 717.\n'
    'C# complete suites: 2630 per platform, all three passing.\n'
    'Both real-etcd configurations pass public native claims and actual-WASM\n'
    'staging, stale prepared-publication refusal, burned suffixes and continuing counters.\n'
    'Rust Windows now passes the exact stopped-server test that previously failed\n'
    'with Winsock 10055. No socket retry, skip or timeout change was made. This\n'
    'passing control does not identify the previous OS resource shortage.\n'
    'Dependencies rejected v2.87.33 after v2.88.0 was published. The reviewed\n'
    'refresh is recorded in install-action-refresh.txt and needs fresh CI.\n'
    'Whole native/C#/linked-WASM 100% and wider unmet R1-R12 criteria remain\n'
    'unfinished. PR139 stays draft and unmerged.\n')
destination = root.parents[1] / 'dev/log/issues/138/pulls/139/author-fence-ci.txt'
destination.write_text(''.join(parts))
