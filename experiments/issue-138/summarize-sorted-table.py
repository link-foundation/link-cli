#!/usr/bin/env python3
"""Check complete physical sorted-table reports without excluding source files."""
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
prefix = 'sorted-table'
print('Physical sorted-table validation; https://github.com/link-foundation/link-cli/pull/139')
print('Published baseline: 9a9a95cda375395a54005a5f6a8495e9eee2fde9.')
print('New optional sealed sorted-table API; independent native binary vectors and SQL-free devices.')
print('Reproduce: bash experiments/issue-138/validate-sorted-table.sh')
driver = (base / f'{prefix}-validation-progress.log').read_text()
assert re.search(r'PASS repository; ALL PASSED \S+', driver), driver[-2048:]
print(driver.strip())
print('Sequential compilers: Cargo jobs1/reduced debug, .NET640MiB, actual-WASM max256MiB.')
for title, stem, expected in [('Native Rust', 'native', 1073), ('Actual WASM', 'wasm', 672)]:
    log = (base / f'{prefix}-{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    assert results and sum(int(p) for p, _, _ in results) == expected and all(int(f) == 0 for _, f, _ in results)
    print(f'\n{title}: {expected} passed; {sum(int(i) for _, _, i in results)} child entries ignored by direct discovery.')
    report = json.loads((base / f'{prefix}-{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions'):
        value = report['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    for item in report['files']:
        if item['filename'].endswith(('/engine/paged/sorted_table.rs',)):
            print('Component ' + item['filename'].split('/rust/', 1)[-1])
            for metric in ('lines', 'branches', 'functions'):
                value = item['summary'][metric]
                print(f"{metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', (base / f'{prefix}-doctests.log').read_text())[-1])
for mode in ('Debug', 'Release'):
    log = (base / f'{prefix}-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2] == '2349'
    coverage = ET.parse(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1]).getroot()
    print(f'\nC# {mode}: {match[1]} passed; zero failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
for title, suffix, expected in [('JS', 'js-full', 533), ('Repository', 'repository', 221)]:
    log = (base / f'{prefix}-{suffix}.log').read_text()
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', log)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] == expected and totals['fail'] == totals['skipped'] == 0
    if title == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
    print(title + ': ' + str(totals))
bun = (base / f'{prefix}-bun.log').read_text()
deno = (base / f'{prefix}-deno.log').read_text()
assert re.search(r'\b174 pass\b', bun) and re.search(r'\b0 fail\b', bun)
assert '174 passed' in deno and '0 failed' in deno
print('Bun/Deno: all 174 selected tests each; strict TypeScript and JS 100% production coverage pass.')
print('Formatting, strict Clippy, SQLite optionality, source limits, changesets, actionlint and ShellCheck pass.')
interop = (base / f'{prefix}-interop.log').read_text()
assert len(re.findall(r'^Verified physical table: ', interop, re.M)) == 25, interop[-2048:]
print('Native Rust/C#/Node/Bun/Deno: all 25 writer/reader pairs pass.')
print('Every reader preserves three physical file hashes and releases ownership.')
vectors = json.loads((base.parent.parent / 'docs/protocol/native-sorted-table-v1.json').read_text())
print(f'Independent binary vectors: {len(vectors["cases"])} valid/corrupt cases; both oracle fixtures regenerate identically.')
print('Missing API failures were preserved before implementation; runnable before-driver uses baseline 9a9a95c.')
print('Shared page-reader retains the existing paged-log path and all existing tests.')
print('Checkpoint CI correction preserves all 258 actual native process cutpoints in 36 cases.')
print('Prior 9a9a95c Windows case deadline failure: run40416245668 log26794-26810; fresh CI is required.')
print('A physical immutable table alone is not an on-disk LSM: WAL/memtable/manifest/compaction composition remains open.')
print('Whole-project coverage and all wider unmet R1-R12 criteria remain unfinished.')
