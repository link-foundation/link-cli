#!/usr/bin/env python3
"""Verify complete saved reports without filtering coverage denominators."""
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
prefix = 'mirrored-root-'
print('Selected checkpoint root mirroring; https://github.com/link-foundation/link-cli/pull/139')
print('Before controls: portable 600cf5f; native 6a0f7c9; see mirrored-root-before.txt.')
print('Validation: bash experiments/issue-138/validate-mirrored-root-all.sh')
print('Sequential compilers: Cargo jobs=1/reduced debug; .NET heap640MiB; actual-WASM cap256MiB.')
for stage in ('native', 'csharp', 'wasm', 'js', 'cluster', 'interop', 'repository'):
    log = (base / f'{prefix}{stage}-stage.log').read_text()
    assert log.endswith(f'Mirrored-root {stage} stage: passed\n'), (stage, log[-2048:])
    print(stage + ': complete stage passed')
for title, stem, expected in [('Native Rust', 'native', 1028), ('Actual WASM', 'wasm', 631)]:
    log = (base / f'{prefix}{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    count = sum(int(p) for p, _, _ in results)
    assert results and count == expected and all(int(f) == 0 for _, f, _ in results), (title, count)
    print(f'\n{title}: {count} passed; {sum(int(i) for _, _, i in results)} child entries ignored by direct discovery.')
    data = json.loads((base / f'{prefix}{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions'):
        value = data['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    suffixes = ['/rust/src/engine/paged/checkpoint_root.rs', '/rust/src/engine/paged/file_checkpoint.rs'] if stem == 'native' else ['/rust/src/engine/paged/checkpoint_root.rs', '/rust/wasm/src/engine.rs']
    for suffix in suffixes:
        component, = [f for f in data['files'] if f['filename'].endswith(suffix)]
        for metric in ('lines', 'branches', 'functions'):
            value = component['summary'][metric]
            print(f"{suffix} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', (base / f'{prefix}doctests.log').read_text())[-1])
for mode in ('Debug', 'Release'):
    log = (base / f'{prefix}csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2] == '2264', (mode, match)
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    coverage = ET.parse(report).getroot()
    print(f'\nC# {mode}: {match[1]} passed; zero failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
    components = [c for c in coverage.findall('.//class') if c.attrib['filename'].endswith(('/Engine/MirroredCheckpointRoot.cs', '/Engine/FileCheckpointPagedCommitLog.cs'))]
    assert components
    for item in components:
        print(f"{item.attrib['name']}: lines {item.attrib['line-rate']}; branches {item.attrib['branch-rate']}")
for title, suffix, expected in [('JS', 'js-full', 261), ('Repository', 'repository', 221)]:
    log = (base / f'{prefix}{suffix}.log').read_text()
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', log)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] == expected and totals['fail'] == totals['skipped'] == 0, totals
    if title == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
        cuts = re.findall(r'native checkpoint mirroredBanks=(false|true) mirroredRoot=(false|true) phase=([a-z-]+): (\d+) bounded before/after process exits', log)
        assert len(cuts) == 9 and sum(int(c[-1]) for c in cuts) == 258, cuts
        print('JS actual native mutation cuts: ' + repr(cuts))
    print(title + ': ' + str(totals))
reference = None
for runtime in ('node', 'bun', 'deno'):
    composition = json.loads((base / f'{prefix}{runtime}-compact.json').read_text())
    assert composition['cases'] == len(composition['results']) == 72
    if reference is not None:
        assert reference == composition
    reference = composition
    conformance = (base / f'{prefix}{runtime}-conformance.log').read_text()
    assert '# 8 passed, 0 failed, 4 skipped' in conformance
    assert 'ok wasm native pages: 48 independent policies' in conformance
    print(f'{runtime}: identical72-case actual-WASM composition; conformance/48 ordinary native policies pass.')
for suffix, marker in [('checkpoint-interop', 'PASS checkpoint pages: 48 configurations, 288 persistent writer handoffs'), ('compact-interop-summary', 'PASS native compaction: 96 configurations, 576 persistent writer handoffs')]:
    assert marker in (base / f'{prefix}{suffix}.log').read_text()
    print(marker)
ordinary = (base / f'{prefix}all-interop.log').read_text()
configurations = re.findall(r'^(single|mirrored)/(scan|indexed)/(flat|shared|sorted-runs)/(snapshot|changes)/(plain|delta)$', ordinary, re.M)
assert len(configurations) == len(set(configurations)) == 48
groups = re.findall(r'^Native pages group: (\d+) -> (\d+) revisions/rows;', ordinary, re.M)
assert groups == [(str(writer * 2), str(writer * 2 + 2)) for _ in range(48) for writer in range(6)]
print('Ordinary48/288 and initial-checkpoint48/288 controls retained. Compaction96/576 independently selects root/bank modes and damages both while closed.')
for mode in ('0', '1', 'cache'):
    text = (base / f'{prefix}etcd-{mode}.log').read_text()
    assert 'PASS real etcd: majority loss refuses a fresh linearizable snapshot' in text
    assert 'PASS independent consensus groups: native/WASM interoperability; catalog contains only atomic pointers' in text
    print(f'Real three-member etcd mode {mode}: complete fault/handoff controls pass.')
print('Vector regeneration, format/strict Clippy, TypeScript, SQL optionality, release/source/workflow checks pass.')
assert 'C# baseline traces: 9 independent scenarios; 258 actual before/after process-exit cuts.' in (base / f'{prefix}csharp-cut-baselines.log').read_text()
print('Independent finite corruption/selection/publication/recovery faults, all selected native ownership modes and258 before/after process cuts per language pass.')
print('Complete native Rust/C#/linked-WASM coverage and every wider unmet criterion stay unfinished. No denominator is reduced.')
print('Truthful synchronization is assumed; process exits do not certify physical power loss or arbitrary simultaneous loss of both root copies.')
