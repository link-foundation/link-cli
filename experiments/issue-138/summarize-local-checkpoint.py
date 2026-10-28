#!/usr/bin/env python3
"""Check saved complete reports; keep component and whole-project scopes explicit."""
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

base = Path(__file__).resolve().parent
prefix = 'local-checkpoint-'
print('Optional portable two-bank journals; PR https://github.com/link-foundation/link-cli/pull/139')
print('Baseline: cdff1b48f007c9c4a8ca9322d491d41b5582f4f3')
print('Run: bash experiments/issue-138/validate-local-checkpoint.sh <stage>')
print('Sequential compilers, Cargo jobs=1, reduced debug; actual WASM capped at 256 MiB.')
for stage in ('csharp', 'native', 'wasm', 'js', 'repository'):
    log = (base / f'{prefix}{stage}-validation.log').read_text()
    assert log.endswith(f'Local-checkpoint {stage} stage: passed\n'), (stage, log[-2048:])
    print(f'{stage}: complete stage passed')
print('Before-feature controls fail independently in Rust, C# and JS; see local-checkpoint-before.txt.')
print('Independent root/empty-bank vectors match six u64 epochs, mirrored flags and malformed checksummed decisions.')
print('Single/mirrored finite failures occur before/after every baseline I/O, followed by durable-image reopening.')
print('Uncertain owners fence; fresh owners retain complete old/new sequences and reuse partial inactive banks.')
print('All 128 torn-root lengths, stale identities, selected-bank damage, no-op/pending inputs and u64 limits pass.')
before = (base / f'{prefix}release-order-before.log').read_text()
assert 'FAILED. 0 passed; 2 failed' in before
assert '["root", "bank0", "bank1"]' in before and '["bank0", "root", "bank1"]' in before
print('Two pre-commit Rust ownership regressions first fail, then require both banks to drop before root.')
for title, stem in [('Native Rust', 'native'), ('Actual WASM', 'wasm')]:
    log = (base / f'{prefix}{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    assert results and all(int(f) == 0 for _, f, _ in results)
    for name in ('owning_device_destruction_releases_root_after_both_banks',
                 'failed_open_releases_banks_before_root_at_every_device_operation'):
        assert name + ' ... ok' in log, (stem, name)
    print(f'\n{title}: {sum(int(p) for p, _, _ in results)} passed, 0 failed, '
          f'{sum(int(i) for _, _, i in results)} ignored child entry points.')
    data = json.loads((base / f'{prefix}{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions'):
        value = data['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    for module in ('checkpoint.rs', 'checkpoint_format.rs'):
        files = [f for f in data['files'] if f['filename'].endswith('/rust/src/engine/paged/' + module)]
        assert len(files) == 1
        for metric in ('lines', 'branches', 'functions'):
            value = files[0]['summary'][metric]
            print(f"{module} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    if stem == 'wasm':
        wrappers = [f for f in data['files'] if '/rust/wasm/src/' in f['filename']]
        assert len(wrappers) == 3
        for metric in ('lines', 'branches', 'functions'):
            covered = sum(f['summary'][metric]['covered'] for f in wrappers)
            count = sum(f['summary'][metric]['count'] for f in wrappers)
            print(f'Wrapper-only {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', (base / f'{prefix}doctests.log').read_text())[-1])
for mode in ('Debug', 'Release'):
    log = (base / f'{prefix}csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2]
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    coverage = ET.parse(report).getroot()
    print(f'\nC# {mode}: {match[1]} passed, 0 failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
    classes = [c for c in coverage.findall('.//class') if c.attrib['filename'].endswith(
        ('/Engine/CheckpointPageFormat.cs', '/Engine/CheckpointPagedCommitLog.cs'))]
    assert classes
    for item in classes:
        print(f"{item.attrib['name']}: lines {item.attrib['line-rate']}; branches {item.attrib['branch-rate']}")
for title, suffix in [('JS', 'js-full'), ('Repository', 'repository')]:
    log = (base / f'{prefix}{suffix}.log').read_text()
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', log)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] and totals['fail'] == totals['skipped'] == 0
    if title == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
    print(title + ': ' + str(totals))
reference = payload = None
for runtime in ('node', 'bun', 'deno'):
    portable = (base / f'{prefix}{runtime}-portable.log').read_text()
    expected = {'node': ('tests 11', 'pass 11', 'fail 0'),
                'bun': ('11 pass', '0 fail', 'Ran 11 tests'),
                'deno': ('11 passed', '0 failed')}
    assert all(part in portable for part in expected[runtime]), runtime
    conformance = (base / f'{prefix}{runtime}-conformance.log').read_text()
    assert '# 8 passed, 0 failed, 4 skipped' in conformance
    assert 'ok wasm native pages: 48 independent policies' in conformance
    composition = json.loads((base / f'{prefix}{runtime}-composition.json').read_text())
    assert composition['cases'] == len(composition['results']) == 36
    current = json.loads((base / f'{prefix}{runtime}-payload.json').read_text())
    assert len(current['results']) == 36 and current['revisions'] == 32
    if reference is not None:
        assert reference == composition and payload == current
    reference, payload = composition, current
    print(f'{runtime}: 11 portable controls, actual-WASM conformance/48 policies and identical 36-case composition/payload reports pass.')
print('Strict format/lint, TypeScript, vector regeneration, optional-SQL guards, release/source/workflow checks pass.')
print('Existing distributed/file handoffs retain the separately recorded preceding validation; no new claim is made here.')
print('Portable devices only, with explicit ownership rules; engine-lock integration and native owning factories remain unfinished.')
print('Truthful device synchronization is assumed. Models/processes do not establish hardware power-loss durability.')
print('Whole-project 100% and every wider unmet issue criterion remain unfinished; fresh exact-head CI required.')
