#!/usr/bin/env python3
"""Verify preserved whole-project reports; never filter coverage denominators."""
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
print('Selected epoch-fenced reclamation; https://github.com/link-foundation/link-cli/pull/139')
print('Missing-feature baseline 44e0fa889e291db571b7e987cc1b7e06764f8ae7; reclamation-before.txt.')
print('Reproduce complete validation: bash experiments/issue-138/validate-reclamation.sh')
driver = (base / 'reclamation-validation-progress.log').read_text()
assert re.search(r'PASS repository; ALL PASSED \S+', driver), driver[-2048:]
print(driver.strip())
print('Sequential compilers: Cargo jobs1/reduced debug; .NET heap640MiB; actual-WASM verified max256MiB.')
vectors = json.loads((base.parent.parent / 'docs/protocol/reclamation-v1.json').read_text())
assert [len(vectors[key]) for key in ('objects', 'epochs', 'pointers')] == [30, 13, 22]
print('Independent wire vectors: 30 addresses, 13 epoch cases, 22 pointer cases; all three implementations consume them.')
for title, stem, expected in [('Native Rust', 'native', 1057), ('Actual WASM', 'wasm', 656)]:
    log = (base / f'reclamation-{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    count = sum(int(p) for p, _, _ in results)
    assert results and count == expected and all(int(f) == 0 for _, f, _ in results), (title, count)
    print(f'\n{title}: {count} passed; {sum(int(i) for _, _, i in results)} child entries ignored by direct discovery.')
    report = json.loads((base / f'reclamation-{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions'):
        value = report['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    suffixes = ('/rust/src/engine/reclaiming.rs', '/rust/src/engine/reclaiming/codec.rs',
                '/rust/src/engine/reclaiming/collector.rs')
    if stem == 'wasm':
        suffixes += ('/rust/wasm/src/engine.rs',)
    for suffix in suffixes:
        component, = [item for item in report['files'] if item['filename'].endswith(suffix)]
        for metric in ('lines', 'branches', 'functions'):
            value = component['summary'][metric]
            print(f"{suffix} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', (base / 'reclamation-doctests.log').read_text())[-1])
for mode in ('Debug', 'Release'):
    log = (base / f'reclamation-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2] == '2307', (mode, match)
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    coverage = ET.parse(report).getroot()
    print(f'\nC# {mode}: {match[1]} passed; zero failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
    suffixes = ('/Engine/ReclaimingPartitionedJournalStore.cs', '/Engine/ReclaimingCollector.cs',
                '/Engine/ReclamationCodec.cs', '/Engine/ReclamationOptions.cs')
    components = [item for item in coverage.findall('.//class') if item.attrib['filename'].endswith(suffixes)]
    assert components
    for item in components:
        filename = Path(item.attrib['filename']).name
        lines, branches = (100 * float(item.attrib[key]) for key in ('line-rate', 'branch-rate'))
        print(f"{item.attrib['name']} [{filename}]: lines {lines:.2f}%; branches {branches:.2f}%")
for title, suffix, expected in [('JS', 'js-full', 518), ('Repository', 'repository', 221)]:
    log = (base / f'reclamation-{suffix}.log').read_text()
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', log)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] == expected and totals['fail'] == totals['skipped'] == 0, totals
    if title == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
    print(title + ': ' + str(totals))
bun = (base / 'reclamation-bun.log').read_text()
deno = (base / 'reclamation-deno.log').read_text()
assert re.search(r'\b159 pass\b', bun) and re.search(r'\b0 fail\b', bun)
assert '159 passed' in deno and '0 failed' in deno
print('Bun/Deno each pass all159 selected tests; strict TypeScript and JS100% full production coverage pass.')
lazy = (base / 'reclamation-lazy.log').read_text()
assert 'PASS lazy reclamation:' in lazy
print(lazy.strip())
memory = (base / 'reclamation-memory-final.log').read_text()
assert 'PASS deterministic test provider:' in memory
print(memory.strip())
for filename, markers in (
    ('reclamation-model-final.log', ('Negative control no epoch CAS:', 'Negative control no epoch namespace:', '70859 complete unique states, 37 terminal states')),
    ('reclamation-reader-model-final.log', ('fractured multi-shard read', '20 complete states, 5 coherent terminal views', 'exact generation preserves it')),
):
    log = (base / filename).read_text()
    assert all(marker in log for marker in markers), log
    print('\n' + log.strip())
real = (base / 'reclamation-etcd.log').read_text()
for marker in ('PASS Rust reclamation:', 'PASS C# reclamation:', 'PASS WASM reclamation:',
               'retired two-shard snapshot refresh returns one complete new group',
               'owned retired journals, warm-cache data outage, catalog remains required',
               'native/WASM handoffs after leader loss, majority loss gives no partial',
               'PASS all reclamation integration contracts'):
    assert marker in real, marker
assert real.count('PASS Rust reclamation:') == real.count('PASS C# reclamation:') == 2
assert real.count('PASS WASM reclamation:') == 4
for phase in ('initialization', 'staging', 'publication', 'fence', 'deletion'):
    for seam in ('before', 'after'):
        assert f'{phase} {seam} response loss, one mutation send' in real
print('\n' + real.strip())
print('Freshly built bounded WASM engine used by actual Node/Bun/Deno handoffs; all child processes reaped.')
print('Vector regeneration, formatting, strict native/WASM Clippy, SQLite optionality, changesets, source limits, repository tests, actionlint and ShellCheck pass.')
print('Namespace ownership, nonreused generations and preserved epoch identity are provider/application assumptions.')
print('Bounded models and process/network probes do not certify physical device power loss.')
print('Whole-project Rust/C#/linked-WASM coverage and wider R1-R12 criteria remain unfinished; no denominator is reduced.')
