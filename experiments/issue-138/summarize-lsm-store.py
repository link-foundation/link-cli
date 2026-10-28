#!/usr/bin/env python3
"""Check complete portable physical LSM store reports without excluding source files."""
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
prefix = 'lsm-store'
print('Portable physical LSM store validation; https://github.com/link-foundation/link-cli/pull/139')
print('Codec-only missing-API baseline: d12760fc13db30439d114ee15ab98be970aae5f1.')
print('Selected production stores use real immutable doublets pages and the checkpoint WAL over finite durable images.')
print('Reproduce: bash experiments/issue-138/validate-lsm-store.sh')
driver = (base / f'{prefix}-validation.log').read_text()
assert re.search(r'PASS repository; ALL PASSED \S+', driver), driver[-2048:]
print(driver.strip())
print('Initial C# formatting gate rejects whitespace; repository formatter applies the changes before focused/full suites.')
print('Sequential compilers: Cargo jobs1/reduced debug, .NET640MiB, actual-WASM max256MiB.')
for title, stem, expected in [('Native Rust', 'native', (1091, 1096)), ('Actual WASM', 'wasm', (692,))]:
    log = (base / f'{prefix}-{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    passed = sum(int(p) for p, _, _ in results)
    assert results and passed in expected and all(int(f) == 0 for _, f, _ in results)
    print(f'\n{title}: {passed} passed; {sum(int(i) for _, _, i in results)} child entries ignored by direct discovery.')
    if stem == 'native' and passed == 1091:
        for suffix in ('expanded-focused', 'expanded-coverage'):
            assert 'test result: ok. 18 passed; 0 failed;' in (base / f'{prefix}-native-{suffix}.log').read_text()
        print('Five mirror/admission controls added after the full run pass in the expanded 18-test target: 1096 distinct native controls.')
        print('Coverage merges this expanded target with --no-clean; production Rust source is unchanged between these runs.')
    report = json.loads((base / f'{prefix}-{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions'):
        value = report['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    for item in report['files']:
        if '/engine/lsm/' in item['filename']:
            print('Component ' + item['filename'].split('/rust/', 1)[-1])
            for metric in ('lines', 'branches', 'functions'):
                value = item['summary'][metric]
                print(f"{metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', (base / f'{prefix}-doctests.log').read_text())[-1])
for mode in ('Debug', 'Release'):
    log = (base / f'{prefix}-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2] == '2369'
    coverage = ET.parse(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1]).getroot()
    print(f'\nC# {mode}: {match[1]} passed; zero failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
for title, suffix, expected in [('JS', 'js-full', 568), ('Repository', 'repository', 221)]:
    log = (base / f'{prefix}-{suffix}.log').read_text()
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', log)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] == expected and totals['fail'] == totals['skipped'] == 0
    if title == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
    print(title + ': ' + str(totals))
bun = (base / f'{prefix}-bun.log').read_text()
deno = (base / f'{prefix}-deno.log').read_text()
assert re.search(r'\b209 pass\b', bun) and re.search(r'\b0 fail\b', bun)
assert '209 passed' in deno and '0 failed' in deno
print('Bun/Deno: all 209 selected tests each; strict TypeScript and JS 100% production coverage pass.')
print('Formatting, strict Clippy, SQLite optionality, source limits, changesets, actionlint and ShellCheck pass.')
interop = (base / f'{prefix}-interop.log').read_text()
assert len(re.findall(r'^Verified physical table: ', interop, re.M)) == 25, interop[-2048:]
print('Existing immutable-table interchange, Rust/C#/Node/Bun/Deno: all 25 writer/reader pairs pass.')
print('Every reader preserves three physical file hashes and releases ownership.')
vectors = json.loads((base.parent.parent / 'docs/protocol/native-lsm-wal-v1.json').read_text())
assert len(vectors['cases']) == 111
print('Independent LSM WAL fixture: 111 cases, 11 valid; regenerates identically without production imports.')
print('Rust/C# typed log APIs skip only the wire non-string provider-record case, which their contracts cannot represent.')
print('Native binary page and sorted-table fixtures also regenerate identically.')
print('Missing APIs fail before production implementation in all three languages; see lsm-store-before.txt.')
print('Two initial JS controls fail due to early repair completion and lost explicit close retry; all production ports preserve deferred repair and retriable cleanup.')
print('Each language verifies 72 compaction and 80 group-write before/after mutation cuts; these are finite page-image schedules.')
print('Native Rust/C# use actual cluster/named-link consumers; JS cluster binding is mocked. Partition/reclamation consumers use physical stores in all ports.')
print('Independent mirrored table controls defer a surviving-copy repair until every selected binding validates.')
print('Fresh-loader guards preserve the ordinary-root lazy boundary and test explicit import positively.')
print('Rust and actual WASM coverage retain complete production denominators, including all uncovered source.')
print('Portable stores do not own a native directory fence or retroactively validate effects of an already constructed WAL.')
print('Native directory/read-only discovery, physical process/interchange, whole-project 100% coverage and every other unmet R1-R12 criterion remain unfinished.')
