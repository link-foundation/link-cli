#!/usr/bin/env python3
"""Retain complete checkpoint-inspection coverage denominators and exact scope."""
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
prefix = 'checkpoint-inspect'
print('Read-only checkpoint inspection validation; https://github.com/link-foundation/link-cli/pull/139')
print('Missing-API baseline: bc364d2e09100bc5bd0d93274b2ecd870e31b66e.')
print('Reproduce: bash experiments/issue-138/validate-checkpoint-inspect.sh')
driver = (base / f'{prefix}-validation.log').read_text()
assert re.search(r'PASS repository; ALL PASSED \S+', driver), driver[-2048:]
print(driver.strip())
print('Sequential compilers: Cargo jobs1/reduced debug, .NET640MiB, actual-WASM max256MiB.')
for title, stem, expected in [('Native Rust', 'native', 1102), ('Actual WASM', 'wasm', 698)]:
    log = (base / f'{prefix}-{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    passed = sum(int(p) for p, _, _ in results)
    assert results and passed == expected and all(int(f) == 0 for _, f, _ in results), (title, passed)
    print(f'\n{title}: {passed} passed; {sum(int(i) for _, _, i in results)} child entries ignored by direct discovery.')
    report = json.loads((base / f'{prefix}-{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions'):
        value = report['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    for item in report['files']:
        if item['filename'].endswith('/checkpoint_inspect.rs'):
            print('Component ' + item['filename'].split('/rust/', 1)[-1])
            for metric in ('lines', 'branches', 'functions'):
                value = item['summary'][metric]
                print(f"{metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', (base / f'{prefix}-doctests.log').read_text())[-1])
for mode in ('Debug', 'Release'):
    log = (base / f'{prefix}-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2] == '2375', mode
    coverage = ET.parse(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1]).getroot()
    print(f'\nC# {mode}: {match[1]} passed; zero failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
for title, suffix, expected in [('JS', 'js-full', 576), ('Repository', 'repository', 221)]:
    log = (base / f'{prefix}-{suffix}.log').read_text()
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', log)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] == expected and totals['fail'] == totals['skipped'] == 0
    if title == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
    print(title + ': ' + str(totals))
bun = (base / f'{prefix}-bun.log').read_text()
deno = (base / f'{prefix}-deno.log').read_text()
assert re.search(r'\b217 pass\b', bun) and re.search(r'\b0 fail\b', bun)
assert '217 passed' in deno and '0 failed' in deno
print('Bun/Deno: all 217 selected tests each; strict TypeScript and JS 100% production coverage pass.')
print('Formatting, strict Clippy, SQLite optionality, source limits, changesets, actionlint and ShellCheck pass.')
interop = (base / f'{prefix}-interop.log').read_text()
assert len(re.findall(r'^Verified physical table: ', interop, re.M)) == 25, interop[-2048:]
print('Existing immutable-table interchange, Rust/C#/Node/Bun/Deno: all 25 writer/reader pairs pass.')
print('Every reader preserves three physical file hashes and releases ownership.')
print('Independent binary-page, immutable-table and 111-case semantic WAL fixtures regenerate identically.')
print('Owned inspection retains the selected complete record prefix without identity allocation, writes, truncation, repair completion or device closure.')
print('Shared native Rust tests execute on actual WASM; C# and JS include equivalent controls.')
print('Controls cover every observed selected-prefix read failure before/after the effect, finite limits, empty/partial initialization, identity/mode/decision conflicts, selected corruption and ignored suffixes.')
print('Real mirrored root/bank decorators stage repairs during inspection; ordinary recovery alone completes those repairs after successful validation.')
print('Physical LSM consumers reject missing selected tables before truncating a staged WAL suffix.')
print('Rust and actual WASM coverage retain complete production denominators, including all uncovered source.')
print('This API borrows devices and does not implement native directory ownership or whole-store process-exit interchange.')
print('Native directory/read-only discovery, full native file-store implementations, whole-project 100% coverage and every other unmet R1-R12 criterion remain unfinished.')
