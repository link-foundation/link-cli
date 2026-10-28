#!/usr/bin/env python3
"""Check complete test-only audit reports without excluding source files."""
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
prefix = 'reclamation-contracts'
print('Reclamation public contract audit; https://github.com/link-foundation/link-cli/pull/139')
print('Published production baseline: 419f7c7c56f68f9d1669658cd4e0b507a90b03cb.')
print('Seven matching public controls per surface; no production correction was needed.')
print('Reproduce: bash experiments/issue-138/validate-reclamation-contracts.sh')
driver = (base / f'{prefix}-validation-progress.log').read_text()
assert re.search(r'PASS repository; ALL PASSED \S+', driver), driver[-2048:]
print(driver.strip())
print('Sequential compilers: Cargo jobs1/reduced debug, .NET640MiB, actual-WASM max256MiB.')
for title, stem, expected in [('Native Rust', 'native', 1064), ('Actual WASM', 'wasm', 663)]:
    log = (base / f'{prefix}-{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    assert results and sum(int(p) for p, _, _ in results) == expected and all(int(f) == 0 for _, f, _ in results)
    print(f'\n{title}: {expected} passed; {sum(int(i) for _, _, i in results)} child entries ignored by direct discovery.')
    report = json.loads((base / f'{prefix}-{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions'):
        value = report['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    for item in report['files']:
        if item['filename'].endswith(('/engine/reclaiming.rs', '/engine/reclaiming/codec.rs', '/engine/reclaiming/collector.rs')):
            print('Component ' + item['filename'].split('/rust/', 1)[-1])
            for metric in ('lines', 'branches', 'functions'):
                value = item['summary'][metric]
                print(f"{metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', (base / f'{prefix}-doctests.log').read_text())[-1])
for mode in ('Debug', 'Release'):
    log = (base / f'{prefix}-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2] == '2314'
    coverage = ET.parse(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1]).getroot()
    print(f'\nC# {mode}: {match[1]} passed; zero failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
for title, suffix, expected in [('JS', 'js-full', 525), ('Repository', 'repository', 221)]:
    log = (base / f'{prefix}-{suffix}.log').read_text()
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', log)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] == expected and totals['fail'] == totals['skipped'] == 0
    if title == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
    print(title + ': ' + str(totals))
bun = (base / f'{prefix}-bun.log').read_text()
deno = (base / f'{prefix}-deno.log').read_text()
assert re.search(r'\b166 pass\b', bun) and re.search(r'\b0 fail\b', bun)
assert '166 passed' in deno and '0 failed' in deno
print('Bun/Deno: all 166 selected tests each; strict TypeScript and JS 100% production coverage pass.')
print('Formatting, strict Clippy, SQLite optionality, source limits, changesets, actionlint and ShellCheck pass.')
print('The unchanged production integration is separately preserved in reclamation.txt and reclamation-ci.txt.')
print('Whole-project coverage and all wider unmet R1-R12 criteria remain unfinished.')
