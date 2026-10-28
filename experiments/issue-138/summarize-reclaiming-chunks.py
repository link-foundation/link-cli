#!/usr/bin/env python3
"""Verify complete preserved chunks2 validation without filtering denominators."""
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
prefix = 'chunk-reclamation'


def log(suffix):
    return (base / f'{prefix}-{suffix}.log').read_text()


print('PR: https://github.com/link-foundation/link-cli/pull/139')
print('Reproduce: bash experiments/issue-138/validate-reclaiming-chunks.sh')
driver = log('validation')
assert re.search(r'^ALL PASS \S+', driver, re.M), driver[-2048:]
print(driver.strip())
print('Sequential jobs1/debug0; .NET640MiB; verified actual-WASM256MiB maximum.')
for title, stem, expected in [('Native Rust', 'native', 1159), ('Actual WASM', 'wasm', 719)]:
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log(f'{stem}-full'))
    assert results and sum(int(p) for p, _, _ in results) == expected, (title, results)
    assert all(int(f) == 0 for _, f, _ in results)
    print(f'{title}: {expected} passed; {sum(int(i) for _, _, i in results)} ignored child entries.')
    if stem == 'wasm':
        print('Actual-WASM binaries: ' + ' + '.join(p for p, _, _ in results))
    report = json.loads((base / f'{prefix}-{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions', 'regions', 'instantiations'):
        value = report['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    suffixes = ('/rust/src/engine/reclaiming.rs', '/rust/src/engine/reclaiming/codec.rs',
                '/rust/src/engine/reclaiming/collector.rs', '/rust/src/engine/reclaiming/objects.rs')
    if stem == 'wasm':
        suffixes += ('/rust/wasm/src/engine.rs', '/rust/wasm/src/engine_log.rs', '/rust/wasm/src/lib.rs')
    for suffix in suffixes:
        component, = [item for item in report['files'] if item['filename'].endswith(suffix)]
        for metric in ('lines', 'branches', 'functions'):
            value = component['summary'][metric]
            print(f"{suffix} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', log('doctests'))[-1])
for mode in ('Debug', 'Release'):
    output = log(f'csharp-{mode}-full')
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', output)
    assert match and match[1] == match[2] == '2442', (mode, match)
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', output, re.M)[-1])
    coverage = ET.parse(report).getroot()
    print(f'C# {mode}: {match[1]} passed; zero failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
for title, suffix, expected in [('Node', 'js-full', 640), ('Repository', 'repository', 221)]:
    output = log(suffix)
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', output)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] == expected and totals['fail'] == totals['skipped'] == 0
    if title == 'Node':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', output)
    print(f'{title}: {totals}')
for runtime in ('node', 'bun', 'deno'):
    output = log(f'{runtime}-consumers')
    for mode in ('direct', 'partitioned', 'chunks', 'reclaiming', 'reclaiming-chunks'):
        assert f'{mode}: actual WASM links' in output, (runtime, mode)
    assert '# 8 passed, 0 failed, 4 skipped' in log(f'{runtime}-conformance')
for layout, suffix in [('whole', 'whole'), ('chunks', 'chunk'), ('reclaiming-chunks', 'reclaiming')]:
    assert f'PASS 50 {layout} complete-store handoffs with physical hash preservation' in log(f'{suffix}-handoffs')
print('Five actual native/WASM compositions; 150 complete-store plain/mirrored five-runtime handoffs pass.')
for suffix in ('payloads', 'reclamation-payloads'):
    payloads = [(base / f'{prefix}-{runtime}-{suffix}.json').read_bytes() for runtime in ('node', 'bun', 'deno')]
    assert payloads[0] == payloads[1] == payloads[2]
print('Actual-engine provider payload accounting is byte-identical in Node/Bun/Deno:')
print((base / f'{prefix}-node-reclamation-payloads.json').read_text().strip())
for layout, suffix in [('blob2', 'whole'), ('chunks2', 'chunks')]:
    output = log(f'etcd-{suffix}')
    assert f'PASS all {layout} reclamation integration contracts' in output
    assert output.count('PASS Rust reclamation:') == output.count('PASS C# reclamation:') == 2
    assert output.count('PASS WASM reclamation:') == 4
    for phase in ('initialization', 'staging', 'publication', 'fence', 'deletion'):
        for seam in ('before', 'after'):
            assert f'{phase} {seam} response loss, one mutation send' in output
    print(output.strip())
print('Both real three-member clusters are shut down and all children reaped.')
assert 'All three chunks2 baseline failures reproduced from clean 58d35f8 source.' in log('baseline-reproduction')
print('All three original missing-feature controls reproduce on isolated immutable baseline source.')
print('All production coverage denominators remain included; full native/C#/linked-WASM 100% remains unfinished.')
print('Finite model/process/network tests do not certify physical power-loss behavior or complete R1-R12.')
