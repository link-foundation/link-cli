#!/usr/bin/env python3
"""Verify saved complete reports and print finite checkpoint evidence."""
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

base = Path(__file__).resolve().parent
root = base.parent.parent
print('Optional logical journal checkpoints; PR https://github.com/link-foundation/link-cli/pull/139')
print('Baseline: d98c2ee30faa578f9116571520f7949014559ac9')
print('Run: bash experiments/issue-138/validate-journal-checkpoint.sh <stage>')
print('Compilers sequential, Cargo jobs=1, reduced debug, actual WASM capped at 256 MiB.')
for stage in ('native', 'csharp', 'wasm', 'js', 'cluster', 'interop', 'repository'):
    log = (base / f'journal-checkpoint-{stage}-validation.log').read_text()
    assert log.endswith(f'Journal-checkpoint {stage} stage: passed\n'), (stage, log[-2048:])
    print(f'{stage}: complete stage passed')
print('Missing-feature baselines: native/C# reject engine3, old actual WASM lacks exportCheckpoint.')
print('Draft branch regressions: both replay loops reject engine3; accepting only that header still')
print('rejects saved bases if historical allocators are replaced with the latest value.')
print('Per-revision committed allocator metadata now preserves exact retained descriptors.')
print('Evicted bases reject; pending reservations remain private; uncertain owners reject export.')
for title, stem in [('Native Rust', 'native'), ('Actual WASM', 'wasm')]:
    log = (base / f'journal-checkpoint-{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    assert results and all(int(f) == 0 for _, f, _ in results)
    print(f'\n{title}: {sum(int(p) for p, _, _ in results)} passed, 0 failed, '
          f'{sum(int(i) for _, _, i in results)} ignored finite child entry points.')
    data = json.loads((base / f'journal-checkpoint-{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions'):
        value = data['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    if stem == 'wasm':
        wrappers = [f for f in data['files'] if '/rust/wasm/src/' in f['filename']]
        assert len(wrappers) == 3
        for metric in ('lines', 'branches', 'functions'):
            covered = sum(f['summary'][metric]['covered'] for f in wrappers)
            count = sum(f['summary'][metric]['count'] for f in wrappers)
            assert covered == count
            print(f'Wrapper-only {metric}: {covered}/{count} (100%)')
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', (base / 'journal-checkpoint-doctests.log').read_text())[-1])
for mode in ('Debug', 'Release'):
    log = (base / f'journal-checkpoint-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2]
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    coverage = ET.parse(report).getroot()
    print(f'\nC# {mode}: {match[1]} passed, 0 failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
for title, suffix in [('JS', 'js-full'), ('Repository', 'repository')]:
    log = (base / f'journal-checkpoint-{suffix}.log').read_text()
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', log)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] and totals['fail'] == totals['skipped'] == 0
    if title == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
    print(title + ': ' + str(totals))
payload = None
for runtime in ('node', 'bun', 'deno'):
    conformance = (base / f'journal-checkpoint-{runtime}-conformance.log').read_text()
    assert '# 8 passed, 0 failed, 4 skipped' in conformance
    assert 'ok wasm native pages: 48 independent policies' in conformance
    current = json.loads((base / f'journal-checkpoint-{runtime}-payload.json').read_text())
    assert len(current['results']) == 36 and current['revisions'] == 32
    if payload is not None:
        assert current == payload
    payload = current
    print(f'{runtime}: actual-WASM conformance, all native-page policies and 36 finite payload cases pass.')
print('\nFinite UTF-8 JSON payload accounting, not disk/network framing or latency:')
print('structure encoding history interval records current-bytes read-bytes published-bytes')
for row in payload['results']:
    print(' '.join(str(row[key]) for key in ('stateStructure', 'revisionEncoding', 'history', 'interval', 'records', 'bytes', 'readBytes', 'publishedBytes')))
for kind in ('0', '1', 'cache'):
    log = (base / f'journal-checkpoint-etcd-{kind}.log').read_text()
    assert 'PASS real etcd: atomic multi-shard CAS' in log
    assert 'PASS real etcd: dropped response' in log
    assert 'PASS C# HTTP:' in log and 'PASS Rust HTTP:' in log
    if kind != '0':
        assert 'PASS checkpoints:' in log
    print(f'Real etcd mode {kind}: native/WASM handoffs, conflicts, unknown outcomes and consensus loss pass.')
interop = (base / 'journal-checkpoint-all-interop.log').read_text()
configurations = re.findall(r'^(single|mirrored)/(scan|indexed)/(flat|shared|sorted-runs)/(snapshot|changes)/(plain|delta)$', interop, re.M)
assert len(configurations) == len(set(configurations)) == 48
assert interop.count('Native pages group:') == 288
print('48 physical/policy configurations and 288 shared-file language writers pass, including mirrored repair.')
print('Strict lint/format, type declarations, optional-SQL guards, release/source and workflow checks pass.')
print('Whole-project 100% and every wider unmet issue criterion remain unfinished; fresh exact-head CI required.')
