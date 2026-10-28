#!/usr/bin/env python3
"""Preserve complete official denominators and finite before/after contracts."""
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

base = Path(__file__).resolve().parent
print('Fallible composition; PR https://github.com/link-foundation/link-cli/pull/139')
print('Baseline production: a1f12fec0e27b7bad8f7d6ba88405f6dc953c5c4')
print('Run: bash experiments/issue-138/validate-fallible-composition.sh <stage>')
print('One compiler job; reduced debug information; actual WASM capped at 256 MiB.')
for name, count in [('file', 10), ('portable', 2)]:
    before = (base / f'fallible-composition-{name}-before.log').read_text()
    assert f'test result: FAILED. 0 passed; {count} failed;' in before
    print(f'Before production changes: {count} {name} regressions fail (exit 101).')
print('C# is a passing exception-propagation control, without production changes.')
print('Returning Err does not undo uncertain writes; caller cleanup remains authoritative.')
for stage in ('csharp', 'native', 'wasm', 'js', 'interop', 'repository'):
    progress = (base / f'fallible-composition-{stage}-validation.log').read_text()
    assert progress.endswith(f'Fallible-composition {stage} stage: passed\n'), (stage, progress)

for scope, stem in [('Native Rust', 'native'), ('Actual WASM', 'wasm')]:
    log = (base / f'fallible-composition-{stem}-full.log').read_text()
    test_log, _, doc_log = log.partition('   Doc-tests link_cli')
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', test_log)
    assert results and all(int(f) == 0 for _, f, _ in results)
    print(f'\n{scope}: {sum(int(p) for p, _, _ in results)} passed, 0 failed; '
          f'{sum(int(i) for _, _, i in results)} ignored child entry points.')
    if doc_log:
        print('Documentation: ' + re.findall(r'test result: ok\.[^\n]+', doc_log)[-1])
    report = base / f'fallible-composition-{stem}-coverage.json'
    data = json.loads(report.read_text())['data'][0]
    summaries = [('Complete production', data['totals'])]
    for name in ['named_type_links.rs', 'persistent_transformations.rs', 'link_reference_validator.rs']:
        summaries.append((name, next(f['summary'] for f in data['files']
                                     if f['filename'].endswith('/' + name))))
    for label, summary in summaries:
        print(label + ':')
        for metric in ('lines', 'branches', 'functions'):
            value = summary[metric]
            print(f"  {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    if stem == 'wasm':
        wrappers = [f for f in data['files'] if '/rust/wasm/src/' in f['filename']]
        assert len(wrappers) == 3
        print('Three wrapper modules, separate from complete linked totals:')
        for metric in ('lines', 'branches', 'functions'):
            covered = sum(f['summary'][metric]['covered'] for f in wrappers)
            total = sum(f['summary'][metric]['count'] for f in wrappers)
            assert covered == total
            print(f'  {metric}: {covered}/{total} (100%)')
    print('Report: ' + str(report.relative_to(base.parent.parent)))
print('Native doctests: ' + re.findall(r'test result: ok\.[^\n]+',
      (base / 'fallible-composition-doctests.log').read_text())[-1])

for mode in ('Debug', 'Release'):
    log = (base / f'fallible-composition-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2], log[-4096:]
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    root = ET.parse(report).getroot()
    print(f'\nComplete C# {mode}: {match[1]} passed, 0 failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, total = (int(root.attrib[f'{metric}-{s}']) for s in ('covered', 'valid'))
        print(f'{metric}: {covered}/{total} ({100 * covered / total:.5f}%)')
    print('Report: ' + str(report.relative_to(base.parent.parent)))

for label, suffix in [('JS', 'js-full'), ('Repository', 'repository')]:
    log = (base / f'fallible-composition-{suffix}.log').read_text()
    totals = {field: int(re.search(r'(?:#|ℹ) ' + field + r' (\d+)\b', log)[1])
              for field in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] and totals['fail'] == totals['skipped'] == 0
    if label == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
    print(f'{label}: {totals}')
for runtime in ('node', 'bun', 'deno'):
    assert 'Actual-WASM sorted-run selection' in (base / f'fallible-composition-{runtime}-contract.log').read_text()
    conformance = (base / f'fallible-composition-{runtime}-conformance.log').read_text()
    assert '# 8 passed, 0 failed, 4 skipped' in conformance
    assert 'ok wasm native pages: 48 independent policies' in conformance
    print(f'{runtime}: actual-WASM public contract and full conformance pass; '
          '8 client scenarios pass, 4 process-only scenarios skip for in-process WASM.')
interop = (base / 'fallible-composition-all-interop.log').read_text()
configurations = re.findall(r'^(single|mirrored)/(scan|indexed)/(flat|shared|sorted-runs)/(snapshot|changes)/(plain|delta)$', interop, re.M)
assert len(configurations) == len(set(configurations)) == 48
assert interop.count('Native pages group:') == 288
assert interop.count('10 -> 12 revisions/rows') == 48
print('48 physical/policy configurations / 288 cross-language file writers pass, including repair.')
print('Ten native real-filesystem regressions pass cleanup, following writes and two exact reopenings.')
print('Four portable contracts cover 11 bookkeeping faults, successful/no-op triggers and parked errors.')
print('Strict Clippy, format, types, release/source-size, optional-SQL guards, ShellCheck and whitespace pass.')
print('Complete 100% coverage and all wider unfinished issue requirements remain active.')
