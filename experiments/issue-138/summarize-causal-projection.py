#!/usr/bin/env python3
"""Verify complete projection receipts without filtering coverage denominators."""
from pathlib import Path
import hashlib
import json
import re
import xml.etree.ElementTree as ET

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]
REPORTS = []


def path(suffix, extension='log'):
    return BASE / f'causal-projection-{suffix}.{extension}'


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
        print('100% production lines/branches/functions with unchanged thresholds and complete denominators.')


print('PR: https://github.com/link-foundation/link-cli/pull/139')
print('Reproduce: bash experiments/issue-138/validate-causal-projection-focused.sh; bash experiments/issue-138/validate-causal-projection.sh')
for suffix, marker in [('js-before', 'ERR_MODULE_NOT_FOUND'), ('native-before', 'E0432'),
                       ('csharp-before', 'CS0246'), ('visibility-before', 'resurrects the losing row')]:
    assert marker in log(suffix), suffix
print('Missing production API controls precede every port; frozen filter-before-select visibility resurrects a losing live value.')
assert 'PASS 128 finite projection scenarios, 3914 input permutation schedules' in log('model')
vectors = ROOT / 'docs/protocol/causal-projection-v1.json'
assert len(json.loads(vectors.read_text())['cases']) == 132
assert vectors.read_bytes() == path('vectors-regenerated', 'json').read_bytes()
print('Independent six-version/two-author/two-row model: 128 scenarios, 3914 permutations and duplicate replays; 132 shared vectors.')
print('Model process limits: 192 MiB address space and 8 MiB stack. Model success alone does not verify production ports.')
for suffix in ('native-focused', 'wasm-focused'):
    assert 'test result: ok. 5 passed; 0 failed; 0 ignored;' in log(suffix)
assert re.search(r'Passed:\s+5, Skipped:\s+0, Total:\s+5', log('csharp-focused'))
assert '5 pass' in log('bun') and '0 fail' in log('bun')
assert '5 passed | 0 failed' in log('deno')
node('node-focused', 5, True)
print('Five portable contracts pass native Rust, C#, Node, Bun, Deno and actual WASM.')
assert re.search(r'^ALL FOCUSED PASS \S+', log('focused-driver'), re.M)
assert re.search(r'^ALL PASS \S+', log('validation'), re.M)
print(log('focused-driver').strip())
print(log('validation').strip())
assert log('dotnet-sdk').strip() == '10.0.403'
for mode in ('Debug', 'Release'):
    output = log(f'csharp-{mode}-full')
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', output)
    assert match and match[1] == match[2] == '2496', (mode, match)
    report_path = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', output, re.M)[-1])
    REPORTS.append(report_path)
    report = ET.parse(report_path).getroot()
    print(f'C# {mode}: 2496 passed, zero failed/skipped, SDK 10.0.403 matching CI.')
    for metric in ('lines', 'branches'):
        covered, count = [int(report.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%).')
    for filename in ('CausalVersion.cs', 'CausalReplica.cs', 'CausalMerkle.cs', 'CausalBatch.cs', 'CausalProjection.cs'):
        classes = [c for c in report.findall('.//class') if c.attrib['filename'].endswith('/' + filename)]
        assert classes, filename
        lines = [line for c in classes for line in c.findall('./lines/line')]
        covered_lines = {int(line.attrib['number']) for line in lines if int(line.attrib['hits']) > 0}
        all_lines = {int(line.attrib['number']) for line in lines}
        branches = [tuple(map(int, re.search(r'\((\d+)/(\d+)\)', line.attrib['condition-coverage']).groups()))
                    for line in lines if 'condition-coverage' in line.attrib]
        print(f'{filename}: lines {len(covered_lines)}/{len(all_lines)}, branches {sum(c for c, _ in branches)}/{sum(n for _, n in branches)}.')
        for cls in classes:
            if cls.attrib['branch-rate'] != '1':
                print(f"Class aggregate {cls.attrib['name']}: branch-rate={cls.attrib['branch-rate']}; includes raw methods omitted from source-line listings.")
        if filename == 'CausalProjection.cs':
            assert len(covered_lines) == len(all_lines)
            assert all(c == n for c, n in branches)
            assert all(cls.attrib['branch-rate'] == '1' for cls in classes)
print('Source-line branch totals do not establish complete compiled branch coverage; the existing Release batch aggregate gap is investigated separately.')
for title, stem, expected in (('Native Rust', 'native', 1231), ('Actual WASM', 'wasm', 757)):
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
    component, = [f for f in report['files'] if f['filename'].endswith('/rust/src/engine/causal_projection.rs')]
    for metric in ('lines', 'branches', 'functions', 'regions', 'instantiations'):
        value = component['summary'][metric]
        print(f"causal_projection.rs {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
assert 'test result: ok. 6 passed; 0 failed; 1 ignored;' in log('doctests')
node('js-full', 686, True)
node('repository', 221)
assert not re.search(r'serde_json|rusqlite|sha2', log('bare-dependencies'))
assert not re.search(r'rusqlite|sha2', log('selected-dependencies'))
assert 'Checked 127 declared versions and the Cargo.lock files.' in log('dependency-freshness')
print('Formats, strict native/WASM Clippy, TS declarations, workflow/ShellCheck, source sizes, release changesets and 127 fresh dependencies/locks pass.')
print('Bare Rust normal dependencies omit JSON/SQL/SHA; selecting causal-projection adds neither SQLite nor SHA. JS uses a separate export.')
print('Complete supplied input is bounded before deduplication/pruning: 1..4096 configured capacity, 1..8 MiB canonical bytes, at most 64 supplied versions per row.')
print('Exact 4096-version and 8 MiB inputs pass; next input rejects. Every supplied identity and equal-clock contradiction is validated before dominance.')
print('Explicit reject or ordinal fixed-width author/numeric-counter maximum policy selects from all retained siblings before visibility; this is not wall-clock order.')
print('Owned versions, selected versions and ordinary rows/names survive caller/returned-value mutation. The projection performs no I/O, publication, allocation or coordination.')
for runtime in ('node', 'bun', 'deno'):
    assert 'PASS owned causal cache: concurrent deletion, explicit resolution, refreshed rows' in log(f'example-{runtime}')
print('The SQL-free physical cache example passes Node, Bun and Deno: concurrent deletion, explicit observed resolution, refreshed rows and unchanged old view.')
print('Deno requires read/write/environment permissions; initial missing executable/permission diagnostics are retained separately. CI runs the Node recipe.')
print('CPU O(V log V + sum(r^2 W)) plus encoding/copying with r<=64; retained space O(VW); every read allocates owned results.')
print('Ordinary mutable sessions and their physical matrix, global allocation, safe membership/history reclamation, generic mapped failures, complete project 100% and all wider unmet criteria remain unfinished.')
print('Fresh publication/platform CI remains required. PR139 stays draft and unmerged.')
suffixes = ('js-before', 'native-before', 'csharp-before', 'visibility-before', 'model', 'focused-driver', 'validation',
            'node-focused', 'js-full', 'native-focused', 'wasm-focused', 'csharp-focused', 'bun', 'deno',
            'csharp-Debug-full', 'csharp-Release-full', 'native-full', 'doctests', 'wasm-full', 'repository',
            'dependency-freshness', 'workflows', 'dotnet-sdk', 'example-node', 'example-bun', 'example-deno',
            'example-deno-path-before', 'example-deno-permissions-before')
sources = ('js/client/src/engine-causal-projection.js', 'js/client/test/engine-causal-projection.test.mjs',
           'rust/src/engine/causal_projection.rs', 'rust/tests/engine_causal_projection_tests.rs',
           'csharp/Foundation.Data.Doublets.Cli.Library/Engine/CausalProjection.cs',
           'csharp/Foundation.Data.Doublets.Cli.Tests/CausalProjectionTests.cs', 'docs/protocol/causal-projection-v1.json',
           'examples/native-lsm-js/projection-cache.mjs')
for report_path in [*REPORTS, *(path(suffix) for suffix in suffixes), *(ROOT / source for source in sources)]:
    data = report_path.read_bytes()
    print(f'{report_path.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
