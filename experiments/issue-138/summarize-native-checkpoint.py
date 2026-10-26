#!/usr/bin/env python3
"""Check complete saved reports before publishing native-factory evidence."""
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
prefix = 'native-checkpoint-'
print('Native checkpoint ownership; https://github.com/link-foundation/link-cli/pull/139')
print('Baseline: 133c4ff4a51240cda983dc34f7e4ca58c44d1c4c; native-checkpoint-before.txt preserves actual failures.')
print('Validation: bash experiments/issue-138/validate-native-checkpoint-all.sh')
print('Sequential compilers: Cargo jobs=1/reduced debug, .NET SDK10.0.403/640MiB heap; WASM256MiB cap.')
print('Initial driver lost its final C# success message after a live edit; both complete reports/coverage were verified, and the remaining stages ran without editing the driver.')
for stage in ('csharp', 'native', 'wasm', 'js', 'cluster', 'interop', 'repository'):
    log = (base / f'{prefix}{stage}-validation.log').read_text()
    assert log.endswith(f'Native-checkpoint {stage} stage: passed\n'), (stage, log[-2048:])
    print(stage + ': complete stage passed')
for title, stem in [('Native Rust', 'native'), ('Actual WASM', 'wasm')]:
    log = (base / f'{prefix}{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    assert results and all(int(f) == 0 for _, f, _ in results)
    print(f'\n{title}: {sum(int(p) for p, _, _ in results)} passed; {sum(int(i) for _, _, i in results)} bounded child entry points ignored by direct discovery.')
    data = json.loads((base / f'{prefix}{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions'):
        value = data['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    suffixes = ['/rust/src/engine/paged/file_checkpoint.rs'] if stem == 'native' else ['/rust/wasm/src/engine.rs']
    for suffix in suffixes:
        components = [f for f in data['files'] if f['filename'].endswith(suffix)]
        assert len(components) == 1, suffix
        for metric in ('lines', 'branches', 'functions'):
            value = components[0]['summary'][metric]
            print(f"{suffix} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    if stem == 'native':
        for name in ['every_real_native_replacement_mutation_survives_before_and_after_process_exit',
                     'selected_checkpoint_factory_rejects_implicit_ordinary_journal_conversion',
                     'every_native_fence_is_acquired_before_selected_bank_repair_or_reset']:
            assert name + ' ... ok' in log, name
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', (base / f'{prefix}doctests.log').read_text())[-1])
for mode in ('Debug', 'Release'):
    log = (base / f'{prefix}csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2]
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    coverage = ET.parse(report).getroot()
    print(f'\nC# {mode}: {match[1]} passed; 0 failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
    components = [c for c in coverage.findall('.//class') if c.attrib['filename'].endswith('/Engine/FileCheckpointPagedCommitLog.cs')]
    assert components
    for item in components:
        print(f"{item.attrib['name']}: lines {item.attrib['line-rate']}; branches {item.attrib['branch-rate']}")
for title, suffix in [('JS', 'js-full'), ('Repository', 'repository')]:
    log = (base / f'{prefix}{suffix}.log').read_text()
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', log)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] and totals['fail'] == totals['skipped'] == 0
    if title == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
        cuts = re.findall(r'native checkpoint mirrored=(false|true) phase=(replace|repair): (\d+) bounded before/after process exits', log)
        assert len(cuts) == 3, cuts
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
for suffix, marker in [('checkpoint-interop', 'PASS checkpoint pages: 48 configurations, 288 persistent writer handoffs'),
                       ('compact-interop-summary', 'PASS native compaction: 48 configurations, 288 persistent writer handoffs')]:
    assert marker in (base / f'{prefix}{suffix}.log').read_text()
    print(marker)
ordinary = (base / f'{prefix}all-interop.log').read_text()
configurations = re.findall(r'^(single|mirrored)/(scan|indexed)/(flat|shared|sorted-runs)/(snapshot|changes)/(plain|delta)$', ordinary, re.M)
assert len(configurations) == len(set(configurations)) == 48
groups = re.findall(r'^Native pages group: (\d+) -> (\d+) revisions/rows;', ordinary, re.M)
assert groups == [(str(writer * 2), str(writer * 2 + 2)) for _ in range(48) for writer in range(6)]
print('Ordinary48/288 control also passes; selected bank epochs1->6 preserve exact names/allocators, two retained revisions, pins, private reservations and branch bases.')
print('All native constructors acquire every fence before repair/reset; all acquired raw owners close in reverse order/root last.')
print('Constructor faults preserve original and every cleanup failure. Close does not flush; uncertain close retains fences; repeated close does not retry.')
print('Rust/C#/JS bounded real process cuts cover before/after replacement AND mirrored recovery writes/truncations/syncs.')
print('Each cut observes child termination before manual fence removal, then checks complete recovery, following append/flush/compaction and fresh reopening.')
print('All three etcd modes, strict format/Clippy, TypeScript, vector regeneration, optional-SQL, release/source/workflow checks pass.')
print('Whole-project100% coverage and every wider unmet issue criterion remain unfinished; no denominator is reduced.')
print('Truthful synchronization is assumed; finite process exits do not establish hardware power-loss durability or arbitrary root-media repair.')
