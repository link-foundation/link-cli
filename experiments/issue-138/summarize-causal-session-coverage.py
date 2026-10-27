#!/usr/bin/env python3
"""Verify focused coverage supplements retain the complete production report."""
import hashlib
import json
from pathlib import Path
import re
import subprocess

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]
PREFIX = BASE / 'causal-session-coverage'
FILES = []


def read(suffix, extension='log'):
    filename = Path(f'{PREFIX}-{suffix}.{extension}')
    FILES.append(filename)
    return filename.read_text()


print('PR: https://github.com/link-foundation/link-cli/pull/139')
print('Production implementation: 082213be4c04c09d16bb24ec9490d36d691d8b51')
print('Reproduce after the complete session run: bash experiments/issue-138/validate-causal-session-coverage.sh')
print('These focused tests supplement the 1241 native/770 actual-WASM complete run; they are not another full-suite run.')
assert 'ALL SUPPLEMENTS PASS ' in read('driver')
assert 'Ran 6 tests' in read('verifier-after') and '\nOK\n' in read('verifier-after')
assert 'FAILED (failures=1)' in read('verifier-before')
print('Six raw verifier contracts pass, after the new complete-session fixture fails before --require-session is implemented.')
print('C# platform CI now requires all six components, including CausalSession, and refuses a report that omits one.')
for mode in ('Debug', 'Release'):
    output = read(f'{mode}-records')
    assert 'CausalSession.cs:' in output and 'PASS every compiled causal branch' in output
    print(output.strip())
for suffix, count in [('native-tests', 10), ('wasm-tests', 10), ('wasm-binding-tests', 3)]:
    output = read(suffix)
    assert f'test result: ok. {count} passed; 0 failed; 0 ignored;' in output
    print(f'{suffix}: {count} passed, zero failed/ignored.')
for stem in ('native', 'wasm'):
    baseline_path = BASE / f'causal-session-{stem}-coverage.json'
    FILES.append(baseline_path)
    baseline = json.loads(baseline_path.read_text())['data'][0]
    report = json.loads(read(stem, 'json'))['data'][0]
    assert {f['filename'] for f in baseline['files']} == {f['filename'] for f in report['files']}
    print(f'{stem}: complete source-file set retained.')
    for metric in ('lines', 'branches', 'functions', 'regions', 'instantiations'):
        old, new = baseline['totals'][metric], report['totals'][metric]
        assert old['count'] == new['count'] and old['covered'] <= new['covered'], (stem, metric)
        print(f"Complete {metric}: {new['covered']}/{new['count']} ({new['percent']:.5f}%); denominator unchanged.")
    components = ['/rust/src/engine/causal_session.rs']
    if stem == 'wasm':
        components.append('/rust/wasm/src/causal_session.rs')
    for name in components:
        component, = [f for f in report['files'] if f['filename'].endswith(name)]
        summary = component['summary']
        assert summary['branches']['covered'] == summary['branches']['count'] == 30
        for metric in ('lines', 'branches', 'functions', 'regions', 'instantiations'):
            value = summary[metric]
            print(f"{name} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
    read(f'{stem}-report')
    read(f'{stem}-clippy')
repository = read('repository')
for key, count in [('tests', 221), ('pass', 221), ('fail', 0), ('cancelled', 0), ('skipped', 0)]:
    assert re.search(r'(?:#|ℹ) ' + key + ' ' + str(count) + r'\b', repository), key
for suffix in ('workflows', 'whitespace'):
    assert not read(suffix).strip()
print('Strict native/WASM all-target Clippy, both formats, 221 repository checks, actionlint/ShellCheck and whitespace pass.')
sources = ['rust/src/engine/causal_session.rs', 'rust/wasm/src/causal_session.rs',
           'rust/tests/engine_causal_session_tests.rs', 'rust/wasm/tests/causal_session_binding.rs',
           '.github/workflows/csharp.yml', '.github/scripts/workflow-policy.test.mjs',
           'experiments/issue-138/check-causal-raw-coverage.py',
           'experiments/issue-138/test-causal-raw-coverage.py',
           'experiments/issue-138/validate-causal-session-coverage.sh',
           'experiments/issue-138/summarize-causal-session-coverage.py']
for source in sources[:2]:
    expected = subprocess.check_output(['git', 'show', '082213b:' + source])
    assert expected == (ROOT / source).read_bytes(), source
print('Production session source bytes are unchanged from the complete run. Added controls cover zero addresses, retained observers, option overflow and unknown operation guards.')
print('Remaining unvisited functions/regions and whole-project coverage remain reported; no exclusions or denominator filtering are added.')
print('Fresh exact-head CI and every wider unfinished R1-R12 criterion remain active. PR139 stays draft and unmerged.')
for filename in [*FILES, *(ROOT / source for source in sources)]:
    data = filename.read_bytes()
    print(f'{filename.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
