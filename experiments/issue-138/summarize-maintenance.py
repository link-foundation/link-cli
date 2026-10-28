#!/usr/bin/env python3
"""Check preserved complete validation reports without coverage filtering."""
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
print('Optional namespace maintenance; https://github.com/link-foundation/link-cli/pull/139')
print('Missing-feature baseline df6e01cbdf08bd11470aac8a72ef1c57a5ca927d; maintenance-before.txt.')
print('Validation: bash experiments/issue-138/validate-maintenance.sh')
print('Sequential Cargo jobs1/reduced debug, .NET heap640MiB, verified actual-WASM cap256MiB.')
driver = (base / 'maintenance-complete-validation.log').read_text()
assert re.search(r'PASS repository; ALL PASSED \S+', driver), driver[-2048:]
print(driver.strip())
vectors = json.loads((base.parent.parent / 'docs/protocol/journal-maintenance-v1.json').read_text())
assert len(vectors['scans']) == 79 and len(vectors['deletes']) == 15
print('Independent wire fixtures: 79 scan / 15 conditional-deletion cases consumed by Rust/C#/JS.')
for title, stem, expected in [('Native Rust', 'native', 1032), ('Actual WASM', 'wasm', 631)]:
    log = (base / f'maintenance-{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    count = sum(int(p) for p, _, _ in results)
    assert results and count == expected and all(int(f) == 0 for _, f, _ in results), (title, count)
    print(f'\n{title}: {count} passed; {sum(int(i) for _, _, i in results)} child entries ignored by direct discovery.')
    report = json.loads((base / f'maintenance-{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions'):
        value = report['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    suffix = '/rust/src/engine/etcd/maintenance.rs' if stem == 'native' else '/rust/wasm/src/engine.rs'
    component, = [item for item in report['files'] if item['filename'].endswith(suffix)]
    for metric in ('lines', 'branches', 'functions'):
        value = component['summary'][metric]
        print(f"{suffix} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', (base / 'maintenance-doctests.log').read_text())[-1])
for mode in ('Debug', 'Release'):
    log = (base / f'maintenance-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2] == '2268', (mode, match)
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    coverage = ET.parse(report).getroot()
    print(f'\nC# {mode}: {match[1]} passed; zero failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
    components = [item for item in coverage.findall('.//class') if item.attrib['filename'].endswith('/Engine/EtcdJournalMaintenance.cs')]
    assert components
    for item in components:
        print(f"{item.attrib['name']}: lines {item.attrib['line-rate']}; branches {item.attrib['branch-rate']}")
for title, suffix, expected in [('JS', 'js-full', 359), ('Repository', 'repository', 221)]:
    log = (base / f'maintenance-{suffix}.log').read_text()
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', log)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] == expected and totals['fail'] == totals['skipped'] == 0, totals
    if title == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
    print(title + ': ' + str(totals))
bun = (base / 'maintenance-bun.log').read_text()
deno = (base / 'maintenance-deno.log').read_text()
assert re.search(r'\b98 pass\b', bun) and re.search(r'\b0 fail\b', bun)
assert '98 passed' in deno and '0 failed' in deno
print('Bun/Deno each pass all98 maintenance cases; strict TypeScript and JS100% full production coverage pass.')
lazy = (base / 'maintenance-lazy.log').read_text()
assert 'PASS lazy maintenance:' in lazy
print(lazy.strip())
real = (base / 'maintenance-etcd.log').read_text()
for marker in ('PASS Rust:', 'PASS C#:', 'PASS JS:', 'revision-pinned pages exclude concurrent', 'compacted historical page restarts', 'lost deletion response', 'complete scan after leader loss', 'majority loss returns no partial', 'PASS all maintenance integration contracts'):
    assert marker in real, marker
assert real.count('PASS JS:') == 3
print('\n' + real.strip())
print('Vector regeneration, formatting, strict native/WASM Clippy, optional-SQL guards, changesets, source limits, repository tests, actionlint and ShellCheck pass.')
print('The existing partitioned store still retains its objects. No object liveness/epoch-fencing collector is implemented by this capability.')
print('Complete native Rust/C#/linked-WASM coverage and wider R1-R12 criteria remain unfinished; no denominator is reduced.')
print('Truthful provider synchronization and nonreused generations are assumptions. These process/network tests do not certify physical power loss.')
