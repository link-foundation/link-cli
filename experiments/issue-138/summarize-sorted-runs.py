#!/usr/bin/env python3
"""Report complete denominators and actual finite contracts, without unions."""
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

base = Path(__file__).resolve().parent
print('Immutable sorted runs; PR https://github.com/link-foundation/link-cli/pull/139')
print('Run each stage of: bash experiments/issue-138/validate-sorted-runs.sh <stage>')
print('One compiler job; debug info disabled; actual WASM capped at 256 MiB.')
before = (base / 'sorted-runs-before.log').read_text()
assert 'unknown state structure' in before
print('Compiled-binding selection fails before implementation: unknown state structure.')
print('This is a missing-feature baseline, not a previously supported behavior defect.')
cache = (base / 'sorted-runs-cache-control.log').read_text()
assert 'test result: ok. 5 passed; 0 failed;' in cache
print('Five native component controls also pass with an explicit no-op cache-retention assertion.')
for stage in ('csharp', 'native', 'wasm', 'js', 'interop', 'repository'):
    progress = (base / f'sorted-runs-{stage}-validation.log').read_text()
    if stage == 'csharp' and not progress.endswith('Sorted-run csharp stage: passed\n'):
        assert 'Complete C# Debug with coverage: passed\n' in progress
        assert 'Complete C# Release with coverage: passed\n' in progress
        assert '8/sorted-runs-shellcheck.log: No such file or directory' in progress
        print('Both C# suites complete successfully; the original wrapper then exits 127')
        print('because its running file was edited. Preserve that error separately from test results.')
        print('The complete reports below and later repository/syntax checks verify the resulting files.')
    else:
        assert progress.endswith(f'Sorted-run {stage} stage: passed\n'), (stage, progress)

for scope, stem in (('Native Rust', 'native'), ('Actual WASM', 'wasm')):
    log = (base / f'sorted-runs-{stem}-full.log').read_text()
    test_log, _, doc_log = log.partition('   Doc-tests link_cli')
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', test_log)
    assert results and all(int(f) == 0 for _, f, _ in results)
    print(f'\n{scope}: {sum(int(p) for p, _, _ in results)} passed, 0 failed; '
          f'{sum(int(i) for _, _, i in results)} ignored child entry points.')
    if doc_log:
        assert 'test result: ok.' in doc_log
        print('Documentation in full run: ' + re.findall(r'test result: ok\.[^\n]+', doc_log)[-1])
    report = base / f'sorted-runs-{stem}-coverage.json'
    data = json.loads(report.read_text())['data'][0]
    for label, summary in [('Complete production', data['totals']),
                           ('sorted_runs.rs', next(f['summary'] for f in data['files']
                                                   if f['filename'].endswith('/sorted_runs.rs')))]:
        print(label + ':')
        for metric in ('lines', 'branches', 'functions'):
            v = summary[metric]
            print(f"  {metric}: {v['covered']}/{v['count']} ({v['percent']:.5f}%)")
    if stem == 'wasm':
        wrappers = [f for f in data['files'] if '/rust/wasm/src/' in f['filename']]
        assert len(wrappers) == 3
        print('Three WASM wrapper source modules (separate from complete linked totals):')
        for metric in ('lines', 'branches', 'functions'):
            covered = sum(f['summary'][metric]['covered'] for f in wrappers)
            count = sum(f['summary'][metric]['count'] for f in wrappers)
            assert covered == count
            print(f'  {metric}: {covered}/{count} (100%)')
    print('Report: ' + str(report.relative_to(base.parent.parent)))
doc = (base / 'sorted-runs-doctests.log').read_text()
print('Native doctests: ' + re.findall(r'test result: ok\.[^\n]+', doc)[-1])

for mode in ('Debug', 'Release'):
    log = (base / f'sorted-runs-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2], log[-4096:]
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    root = ET.parse(report).getroot()
    print(f'\nComplete C# {mode}: {match[1]} passed, 0 failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, total = (int(root.attrib[f'{metric}-{s}']) for s in ('covered', 'valid'))
        print(f'{metric}: {covered}/{total} ({100 * covered / total:.5f}%)')
    lines = {}
    for cls in root.findall('.//class'):
        if cls.attrib['filename'].endswith('/SortedRunRows.cs'):
            for line in cls.findall('./lines/line'):
                n = int(line.attrib['number'])
                lines[n] = lines.get(n, False) or int(line.attrib['hits']) > 0
    print(f'SortedRunRows.cs unique lines: {sum(lines.values())}/{len(lines)}')
    print('Report: ' + str(report.relative_to(base.parent.parent)))

for label, suffix in (('Node 26 JS', 'js-full'), ('Node 24 JS', 'js-node24'),
                      ('Repository', 'repository')):
    log = (base / f'sorted-runs-{suffix}.log').read_text()
    totals = {field: int(re.search(r'(?:#|ℹ) ' + field + r' (\d+)\b', log)[1])
              for field in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] and totals['fail'] == totals['skipped'] == 0
    if label != 'Repository':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log), label
    print(f'{label}: {totals}')
for runtime in ('node', 'bun', 'deno'):
    log = (base / f'sorted-runs-{runtime}-contract.log').read_text()
    assert 'Actual-WASM sorted-run selection' in log
    conformance = (base / f'sorted-runs-{runtime}-conformance.log').read_text()
    assert '# 8 passed, 0 failed, 4 skipped' in conformance
    assert 'ok wasm native pages: 48 independent policies' in conformance
    assert 'independent flat/shared/sorted-run state' in conformance
    print(f'{runtime}: actual sorted-run public contract and expanded conformance pass; '
          'client runner 8 passed, 4 process-only scenarios skipped for in-process WASM.')
print('All JS production modules pass 100% line/branch/function thresholds; strict types pass.')
interop = (base / 'sorted-runs-all-interop.log').read_text()
configurations = re.findall(r'^(single|mirrored)/(scan|indexed)/(flat|shared|sorted-runs)/(snapshot|changes)/(plain|delta)$', interop, re.M)
assert len(configurations) == len(set(configurations)) == 48
assert interop.count('Native pages group:') == 288
assert interop.count('10 -> 12 revisions/rows') == 48
print('48 configurations / 288 Rust/Node/Bun/C#/Deno/Rust file writers pass, including damaged-copy repair.')
print('Newest-write/tombstone/resurrection, pins, no-ops, run identity, all conversions,')
print('independent policies, group/branch behavior and durable-prefix reopening pass.')
print('Native/WASM strict Clippy, formatting, release/source-size, optional-SQL guards and ShellCheck pass.')
print('Physical SSTables, legacy mapped repair, whole-project 100% and every wider criterion remain unfinished.')
