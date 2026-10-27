#!/usr/bin/env python3
"""Check full mutable-session evidence without filtering production coverage."""
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]
REPORTS = []


def path(suffix, extension='log'):
    return BASE / f'causal-session-{suffix}.{extension}'


def log(suffix):
    return path(suffix).read_text()


def node(suffix, expected, coverage=False):
    text = log(suffix)
    counts = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', text)[1])
              for key in ('tests', 'pass', 'fail', 'skipped', 'cancelled')}
    assert counts['tests'] == counts['pass'] == expected, (suffix, counts)
    assert counts['fail'] == counts['skipped'] == counts['cancelled'] == 0
    print(f'{suffix}: {counts}.')
    if coverage:
        assert re.search(r'all files\s+\| 100\.00\s+\|\s+100\.00\s+\|\s+100\.00', text)
        print('Enforced 100% production lines/branches/functions; every production denominator retained.')


print('PR: https://github.com/link-foundation/link-cli/pull/139')
print('Reproduce: bash experiments/issue-138/validate-causal-session.sh')
print('Actual Node/Bun/Deno: bash experiments/issue-138/validate-causal-session-actual.sh')
for suffix, marker in [('js-before', 'ERR_MODULE_NOT_FOUND'), ('native-before', 'E0432'),
                       ('csharp-before', 'CS0246'), ('wasm-binding-before', 'E0432')]:
    assert marker in log(suffix), suffix
print('Missing public APIs fail before implementation in each port and the actual WASM binding.')
assert 'PASS 1029 finite session sequences; 686 exact before/after-reply retries.' in log('model')
vectors = ROOT / 'docs/protocol/causal-session-v1.json'
assert len(json.loads(vectors.read_text())['cases']) == 343
assert vectors.read_bytes() == path('vectors-regenerated', 'json').read_bytes()
print('Independent finite model: 1029 three-operation sequences, 686 before/after lost-reply retries; 192 MiB address space, 8 MiB stack.')
print('343 shared vectors execute normal/before-CAS/after-CAS paths in production Rust/C# and actual Node/Bun/Deno.')
print('Model evidence alone does not verify production; shared vectors independently check expected rows, names, errors and final edits.')
for runtime in ('node', 'bun', 'deno'):
    assert 'PASS actual causal sessions: nine contract groups, 1029 finite schedules and 686 exact before/after-reply retries' in log(f'actual-{runtime}')
assert re.search(r'^ALL ACTUAL PASS \S+', log('actual-driver'), re.M)
print(log('actual-driver').strip())
assert 'Ran 5 tests' in log('raw-verifier') and '\nOK\n' in log('raw-verifier')
assert 'FAILED (failures=2)' in log('raw-windows-before')
print('Five raw Coverlet verifier contracts pass after reproducing Windows path rejection and zero-hit generated methods with no source lines.')
assert 'AssertionError: 6 compiled causal branches remain uncovered' in log('Debug-records-before')
assert 'CausalSession.cs: 102/108 raw branches' in log('Debug-records-before')
print('First complete Debug run passes 2506 tests but the raw check rejects six unvisited session edges; additional zero-address and restriction semantics controls exercise them.')
assert re.search(r'ℹ fail 2\b', log('rejected-commit-before'))
node('rejected-commit-after', 9, True)
print('Rejected JS commit requests previously poisoned immediately following valid commits/retries. Two controls fail before the pending-slot admission correction; all nine pass after it.')
assert 'ℹ fail 1' in log('raw-ci-before') and 'ℹ fail 0' in log('raw-ci-after')
print('A failing workflow policy control precedes enforcing raw compiled causal coverage in every C# platform job; reports remain retained after failures.')
assert re.search(r'^ALL PASS \S+', log('full-driver'), re.M)
print(log('full-driver').strip())
assert log('dotnet-sdk').strip() == '10.0.403'
for mode in ('Debug', 'Release'):
    output = log(f'csharp-{mode}-full')
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', output)
    assert match and match[1] == match[2] == '2506', (mode, match)
    report_path = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', output, re.M)[-1])
    REPORTS.append(report_path)
    report = ET.parse(report_path).getroot()
    print(f'C# {mode}: 2506 passed, zero failed/skipped, SDK 10.0.403 matching CI.')
    for metric in ('lines', 'branches'):
        covered, count = [int(report.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%).')
    records = log(f'{mode}-records')
    assert 'PASS every compiled causal branch, including methods without source lines' in records
    print(records.strip())
    raw, = report_path.parent.glob('coverage.json')
    REPORTS.append(raw)
    for filename in ('CausalVersion.cs', 'CausalReplica.cs', 'CausalMerkle.cs',
                     'CausalBatch.cs', 'CausalProjection.cs', 'CausalSession.cs'):
        classes = [c for c in report.findall('.//class') if c.attrib['filename'].replace('\\', '/').endswith('/' + filename)]
        assert classes, filename
        lines = [line for c in classes for line in c.findall('./lines/line')]
        covered = {int(line.attrib['number']) for line in lines if int(line.attrib['hits']) > 0}
        all_lines = {int(line.attrib['number']) for line in lines}
        assert covered == all_lines, (mode, filename, all_lines - covered)
        print(f'{filename}: source lines {len(covered)}/{len(all_lines)}; raw compiled branches checked separately above.')
for title, stem, expected in (('Native Rust', 'native', 1241), ('Actual WASM', 'wasm', 770)):
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log(f'{stem}-full'))
    assert results and sum(int(p) for p, _, _ in results) == expected, (title, results)
    assert all(int(f) == 0 for _, f, _ in results)
    print(f'{title}: {expected} passed, zero failed; {sum(int(i) for _, _, i in results)} existing parent-driven entries ignored by normal harness.')
    report_path = path(f'{stem}-coverage', 'json')
    REPORTS.append(report_path)
    report = json.loads(report_path.read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions', 'regions', 'instantiations'):
        value = report['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
    filenames = ['/rust/src/engine/causal_session.rs']
    if stem == 'wasm':
        filenames += ['/rust/wasm/src/causal_session.rs']
    for filename in filenames:
        component, = [f for f in report['files'] if f['filename'].endswith(filename)]
        for metric in ('lines', 'branches', 'functions', 'regions', 'instantiations'):
            value = component['summary'][metric]
            print(f"{filename} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
assert 'test result: ok. 6 passed; 0 failed; 1 ignored;' in log('doctests')
node('js-full', 695, True)
node('repository', 221)
assert not re.search(r'serde_json|rusqlite|sha2', log('bare-dependencies'))
assert not re.search(r'rusqlite|sha2', log('selected-dependencies'))
assert 'Checked 127 declared versions and the Cargo.lock files.' in log('dependency-freshness')
print('Formats, strict native/WASM Clippy, TS declarations, workflow/ShellCheck, source sizes, release changesets and 127 fresh dependencies/locks pass.')
print('Bare Rust normal dependencies omit JSON/SQL/SHA; selecting causal-sessions adds neither SQLite nor SHA. JS uses a separate export.')
assert 'PASS 50 session complete-store handoffs with physical hash preservation' in log('physical-handoffs')
assert 'PASS 20 reaped process exits; 100 cross-runtime retry/reopen/following checks' in log('process-faults')
assert re.search(r'^ALL PHYSICAL PASS \S+', log('physical-driver'), re.M)
print(log('physical-driver').strip())
print(log('physical-handoffs').strip())
print(log('process-faults').strip())
print('All physical examples additionally compare every file byte/hash immediately across stale-session commit refusal, before any following publication.')
print('Resource-bounded process exits are application-crash evidence; hardware power loss remains unverified.')
print('Preparation freezes the original complete observations, final diff, generation and token; unknown replies retry exactly after following writes.')
print('No-op sessions persist empty receipts without author counters; invalid token syntax does not freeze; invalid final constraints/token ranges never publish.')
print('Reserved u64 allocation remains separate from the private journal cursor, including remote u64::MAX and lower fresh reservations.')
print('Owned reads/prepared/results and repeated disposal release private resources without undoing completed publication; JS frees the native handle on every preparation outcome.')
print('Projection CPU O(V log V + sum(r^2 W)); owned retained space O(VW); final diff O((R+F) log(R+F)) plus payload copies and selected engine mutation costs.')
print('Publication retains the batch complete-journal replay/copy costs; no incremental replay/throughput or remote group-atomicity claim is inferred.')
print('Full u32 LiNo query parity, global allocation, safe membership/history reclamation, generic mapped failures, complete project 100% and all wider unmet criteria remain unfinished.')
print('Fresh exact-head platform CI remains required. PR139 stays draft and unmerged.')
suffixes = ('js-before', 'native-before', 'csharp-before', 'wasm-binding-before', 'model',
            'actual-driver', 'actual-node', 'actual-bun', 'actual-deno', 'full-driver',
            'js-full', 'csharp-Debug-full', 'csharp-Release-full', 'native-full',
            'doctests', 'wasm-full', 'repository', 'dependency-freshness', 'workflows',
            'dotnet-sdk', 'physical-handoffs', 'process-faults', 'raw-windows-before', 'raw-verifier')
suffixes += ('Debug-records-before', 'rejected-commit-before', 'rejected-commit-after',
             'raw-ci-before', 'raw-ci-after', 'physical-driver')
sources = ('rust/src/engine/causal_session.rs', 'rust/tests/engine_causal_session_tests.rs',
           'rust/wasm/src/causal_session.rs', 'rust/wasm/tests/causal_session_binding.rs',
           'csharp/Foundation.Data.Doublets.Cli.Library/Engine/CausalSession.cs',
           'csharp/Foundation.Data.Doublets.Cli.Tests/CausalSessionTests.cs',
           'js/client/src/engine-causal-session.js', 'js/client/test/engine-causal-session.test.mjs',
           'js/client/test/conformance/causal-session.mjs', 'docs/protocol/causal-session-v1.json',
           'examples/native-lsm-js/session-interop.mjs', 'rust/examples/native_lsm/session.rs',
           'examples/cluster-csharp/CausalSessionInterop.cs')
sources += ('.github/workflows/csharp.yml', '.github/workflows/js-client.yml',
            '.github/scripts/workflow-policy.test.mjs', 'experiments/issue-138/check-causal-raw-coverage.py',
            'experiments/issue-138/test-causal-raw-coverage.py', 'experiments/issue-138/validate-causal-session.sh',
            'experiments/issue-138/validate-causal-session-physical.sh')
for report_path in [*REPORTS, *(path(suffix) for suffix in suffixes), *(ROOT / source for source in sources)]:
    data = report_path.read_bytes()
    print(f'{report_path.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
