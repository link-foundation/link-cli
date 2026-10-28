#!/usr/bin/env python3
"""Record full denominators alongside compiled-method branch completeness."""
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'experiments/issue-138'
REPORTS = []
def log(suffix):
    return (BASE / f'causal-raw-{suffix}.log').read_text()

def node(suffix, count, coverage=False):
    output = log(suffix)
    result = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', output)[1])
              for key in ('tests', 'pass', 'fail', 'skipped', 'cancelled')}
    assert result['tests'] == result['pass'] == count
    assert result['fail'] == result['skipped'] == result['cancelled'] == 0
    if coverage:
        assert re.search(r'all files\s+\| 100\.00\s+\|\s+100\.00\s+\|\s+100\.00', output)
    print(f'{suffix}: {result}; complete production coverage={coverage}.')

print('PR: https://github.com/link-foundation/link-cli/pull/139')
print('Reproduce: bash experiments/issue-138/validate-causal-raw-coverage.sh')
before = (BASE / 'causal-raw-coverage-before.log').read_text()
assert 'source lines=0' in before and "'Offset': 8" in before and "'Path': 1" in before
assert 'CausalBatch.cs: 187/188 raw branches' in before
assert 'AssertionError: 1 compiled causal branches remain uncovered' in before
assert 'ℹ fail 1' in (BASE / 'causal-raw-retention-before.log').read_text()
print('Before: Release generated ParseEdits lambda has no source lines; IL offset 8, path 1, line 61 has zero hits. Raw batch branches 187/188 despite complete source-line branch conditions.')
print('Before: workflow-policy contract fails because JSON collection/retention is absent.')
vectors = json.loads((ROOT / 'docs/protocol/causal-batch-v1.json').read_text())
assert len(vectors['cases']) == 54
assert 'non-array-edit' in json.dumps(vectors)
assert (ROOT / 'docs/protocol/causal-batch-v1.json').read_bytes() == (BASE / 'causal-raw-vectors-regenerated.json').read_bytes()
print('54 independent shared cases preserve every previous vector and add a scalar individual receipt edit. Production code and thresholds are unchanged.')
for mode in ('Debug', 'Release'):
    assert re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+2496, Skipped:\s+0, Total:\s+2496', log(f'csharp-{mode}'))
    raw, = (BASE / f'causal-raw-{mode}-results').glob('*/coverage.json')
    xml = raw.with_name('coverage.cobertura.xml')
    REPORTS.extend([raw, xml])
    report = ET.parse(xml).getroot()
    print(f'C# {mode}: all 2496 tests pass; SDK 10.0.403.')
    for metric in ('lines', 'branches'):
        covered, total = (int(report.attrib[f'{metric}-{key}']) for key in ('covered', 'valid'))
        print(f'Whole-project {metric}: {covered}/{total} ({covered * 100 / total:.5f}%).')
    files = {}
    for documents in json.loads(raw.read_text()).values():
        for filename, classes in documents.items():
            name = Path(filename).name
            if name not in {'CausalBatch.cs', 'CausalMerkle.cs', 'CausalProjection.cs', 'CausalReplica.cs', 'CausalVersion.cs'}:
                continue
            branches = files.setdefault(name, [])
            for methods in classes.values():
                for method in methods.values(): branches.extend(method['Branches'])
    assert len(files) == 5
    for name, branches in sorted(files.items()):
        assert branches and all(branch['Hits'] > 0 for branch in branches)
        print(f'{name}: {len(branches)}/{len(branches)} compiled branches, including all methods with no source lines.')
    assert 'PASS every compiled causal branch' in log(f'{mode}-records')
for suffix in ('native', 'wasm'):
    assert 'test result: ok. 7 passed; 0 failed; 0 ignored;' in log(suffix)
print('Seven shared-vector contracts pass native Rust and actual WASM. These are focused fixture checks; the preceding full projection suite/LLVM reports remain separate historical evidence.')
node('node', 686, True)
assert '15 pass' in log('bun') and '0 fail' in log('bun')
assert '15 passed | 0 failed' in log('deno')
node('repository', 221)
assert not log('workflows').strip() and not log('whitespace').strip()
assert re.search(r'^ALL PASS \S+', log('driver'), re.M)
print(log('driver').strip())
print('Complete Node production lines/branches/functions remain 100%. Bun/Deno pass all 15 batch contracts; repository, actionlint, ShellCheck and whitespace pass.')
print('CI collects and retains both raw JSON and Cobertura reports on all three platforms. A raw component guard does not replace any full-project denominator.')
print('All wider unmet criteria, full-project 100%, mutable sessions and their physical matrix remain unfinished. PR139 stays draft and unmerged.')
REPORTS.extend(BASE / f'causal-raw-{suffix}.log' for suffix in ('coverage-before', 'retention-before', 'driver', 'Debug-records', 'Release-records', 'csharp-Debug', 'csharp-Release', 'native', 'wasm', 'node', 'bun', 'deno', 'repository', 'workflows', 'whitespace'))
REPORTS.extend(ROOT / source for source in ('docs/protocol/causal-batch-v1.json', '.github/workflows/csharp.yml', '.github/scripts/workflow-policy.test.mjs', 'experiments/issue-138/generate-causal-batch-vectors.py', 'experiments/issue-138/check-causal-raw-coverage.py', 'experiments/issue-138/validate-causal-raw-coverage.sh'))
for path in REPORTS:
    raw = path.read_bytes()
    print(f'{path.relative_to(ROOT)}: {len(raw)} bytes; SHA256 {hashlib.sha256(raw).hexdigest()}.')
