#!/usr/bin/env python3
"""Admit exact-head batch workflows and downloaded native platform logs."""
import hashlib
import json
import re
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'experiments/issue-138'
HEAD = '80c41ad3196cb87f4f5291eb16e459bbd741afce'
runs_path = BASE / 'causal-batch-runs-current.json'
runs = json.loads(runs_path.read_text())
assert len(runs) == 9
assert all(run['headSha'] == HEAD and run['status'] == 'completed' and
           run['conclusion'] == 'success' and run['createdAt'] == '2026-10-27T18:18:31Z'
           for run in runs), 'Exact-head workflows are not all complete and passing.'
print('PR: https://github.com/link-foundation/link-cli/pull/139')
print(f'Exact published head: {HEAD}; workflows created 2026-10-27T18:18:31Z.')
for run in sorted(runs, key=lambda item: item['name']):
    print(f"{run['name']}: PASS https://github.com/link-foundation/link-cli/actions/runs/{run['databaseId']}")
for job, platform in [(122382826889, 'macos-26'), (122382827038, 'ubuntu-24.04'),
                      (122382826807, 'windows-2025')]:
    path = BASE / f'ci-logs/causal-batch-rust-{job}.log'
    data = path.read_bytes()
    assert data, f'No downloaded log for {job}'
    groups = defaultdict(lambda: [0, 0, 0, 0])
    command = None
    for line in data.decode('utf-8', errors='replace').splitlines():
        if '##[group]Run ' in line:
            command = line.split('##[group]Run ', 1)[1]
        match = re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', line)
        if match and command and command.startswith('cargo test '):
            for index, value in enumerate(match.groups()):
                groups[command][index] += int(value)
            groups[command][3] += 1
    assert len(groups) == 3, (platform, groups)
    print(f'\n{platform}: https://github.com/link-foundation/link-cli/actions/runs/40558747374/job/{job}')
    for command, (passed, failed, ignored, records) in groups.items():
        assert passed > 0 and failed == 0
        print(f'{command}: {passed} passed, {failed} failed, {ignored} ignored across {records} result records.')
    print(f'{path.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}; {len(data.splitlines())} lines.')
print('\nAll-features includes six passing doctests and one existing ignored doctest; the separate doc command repeats them.')
print('Focused no-default-feature tests are recorded separately and are not added to the full-suite count.')
print('Actual Linux/macOS/Windows C# archives are admitted independently in causal-batch-platform-coverage.txt.')
print('All reported C# source-line branch conditions are covered, but the Release batch class aggregate is below 100%; raw method records are investigated separately.')
print('These workflows validate the published batch implementation, not subsequent projection/session work or all unmet R1-R12 criteria.')
data = runs_path.read_bytes()
print(f'{runs_path.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
