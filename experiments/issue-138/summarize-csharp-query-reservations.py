#!/usr/bin/env python3
"""Preserve finite allocation regressions and complete, unfiltered C# reports."""
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'experiments/issue-138'
PREFIX = 'csharp-query-reservations'
FILES = set()


def read(name):
    path = BASE / name
    FILES.add(path)
    return path.read_text()


print('PR: https://github.com/link-foundation/link-cli/pull/139')
print('Reproduce: bash experiments/issue-138/validate-csharp-query-reservations.sh')
driver = read(f'{PREFIX}-validation.log')
assert 'Debug PASS ' in driver and 'Release PASS ' in driver and 'ALL PASS ' in driver
print(driver.strip())
before = read('csharp-query-allocation-unit-before.log')
assert re.search(r'Failed:\s+10,\s+Passed:\s+0,\s+Skipped:\s+0,\s+Total:\s+10', before)
assert '2 passed; 1 failed' in read('csharp-query-allocation-before.log')
print('Before: all ten permanent allocation cases fail; the independent three-case probe has two passing explicit/pair controls and one dangling anonymous point.')
focused = read(f'{PREFIX}-focused.log')
assert re.search(r'Failed:\s+0,\s+Passed:\s+22,\s+Skipped:\s+0,\s+Total:\s+22', focused)
draft = read(f'{PREFIX}-draft-Debug-tests.log')
assert re.search(r'Failed:\s+1,\s+Passed:\s+2531,\s+Skipped:\s+0,\s+Total:\s+2532', draft)
assert 'EngineReadSetTests.OwnWritesAndBlindAllocationsValidateWrittenAddresses' in draft
print('Draft full-suite regression: checking every allocation for collisions introduced a point-read dependency into blind writes. The existing read-set conflict test fails before limiting the check to reserved addresses.')
print('After: ten reproductions, eleven additional ownership/concurrency/failure/wide-address/preview/compatibility contracts and the existing blind-write regression pass.')
after = read(f'{PREFIX}-allocation-after.log')
assert '3 passed; 0 failed' in after
print(after.strip())
assert 'PASS: shared generic query API' in read(f'{PREFIX}-after.log')
assert '3 passed; 0 failed' in read(f'{PREFIX}-zero-after.log')
print('Selected-address generic and all three zero-binding probes remain passing.')

for mode in ['Debug', 'Release']:
    output = read(f'{PREFIX}-{mode}-tests.log')
    assert re.search(r'Failed:\s+0,\s+Passed:\s+2535,\s+Skipped:\s+0,\s+Total:\s+2535', output)
    results = Path(read(f'{PREFIX}-{mode}-results-path.txt').strip())
    raw, = results.glob('*/coverage.json')
    xml, = results.glob('*/coverage.cobertura.xml')
    FILES.update([raw, xml])
    report = ET.fromstring(xml.read_bytes())
    print(f'{mode}: 2535 passed; zero failed/skipped. Complete production Cobertura:')
    for metric in ['lines', 'branches']:
        covered, total = int(report.attrib[f'{metric}-covered']), int(report.attrib[f'{metric}-valid'])
        print(f'  {metric}: {covered}/{total} ({100 * covered / total:.5f}%).')
    old_results = Path(read(f'csharp-generic-query-{mode}-results-path.txt').strip())
    old, = old_results.glob('*/coverage.cobertura.xml')
    FILES.add(old)
    previous = {c.attrib['filename'] for c in ET.fromstring(old.read_bytes()).iter('class')}
    current = {c.attrib['filename'] for c in report.iter('class')}
    assert previous <= current, sorted(previous - current)
    print(f'  Every previous production file retained: {len(previous)} previous, {len(current)} current.')
    reservation_sources = {'EngineQueryReservations.cs', 'QueryPreview.cs', 'QueryReservations.cs'}
    seen = set()
    for documents in json.loads(raw.read_bytes()).values():
        for filename, classes in documents.items():
            name = Path(filename.replace('\\', '/')).name
            if name not in reservation_sources:
                continue
            seen.add(name)
            lines, branches = {}, []
            for methods in classes.values():
                for evidence in methods.values():
                    for line, hits in evidence['Lines'].items():
                        lines[line] = lines.get(line, 0) + hits
                    branches.extend(evidence['Branches'])
            assert lines and all(lines.values()), name
            assert branches and all(branch['Hits'] > 0 for branch in branches), name
            print(f'  {name}: {len(lines)}/{len(lines)} source lines; {len(branches)}/{len(branches)} raw compiled branches.')
    assert seen == reservation_sources
    raw_result = read(f'{PREFIX}-{mode}-causal-coverage.log')
    assert 'PASS every compiled causal branch' in raw_result
    print(raw_result.strip())
    causal = {'CausalBatch.cs', 'CausalMerkle.cs', 'CausalProjection.cs',
              'CausalReplica.cs', 'CausalSession.cs', 'CausalVersion.cs'}
    for documents in json.loads(raw.read_bytes()).values():
        for filename, classes in documents.items():
            name = Path(filename.replace('\\', '/')).name
            if name not in causal:
                continue
            lines = {}
            for methods in classes.values():
                for evidence in methods.values():
                    for line, hits in evidence['Lines'].items():
                        lines[line] = lines.get(line, 0) + hits
            assert lines and all(lines.values()), name
            print(f'  {name}: all {len(lines)} source lines visited.')

layout_failure = read(f'{PREFIX}-repository-before-relocation.log')
assert 'ci-logs should live under the package-specific project tree or case-study evidence' in layout_failure
print('Repository layout initially rejects downloaded root CI logs; preserved logs moved under experiments/issue-138/ci-logs before all 221 checks pass.')
assert read(f'{PREFIX}-format.log') == ''
for suffix in ['repository', 'source-size', 'changeset', 'shellcheck', 'release-tests', 'whitespace']:
    read(f'{PREFIX}-{suffix}.log')
assert re.search(r'(?:#|ℹ) pass 221\b', read(f'{PREFIX}-repository.log'))
print('Strict full C# format, 221 repository tests, C# source-size/changeset/release-helper, ShellCheck and whitespace gates pass.')
snapshot = json.loads(read('causal-protocol-rust-source-snapshot.json'))
for item in snapshot['files']:
    assert hashlib.sha256((ROOT / item['path']).read_bytes()).hexdigest() == item['sha256'], item['path']
previous_receipt = ROOT / 'dev/log/issues/138/pulls/139/causal-protocol.txt'
FILES.add(previous_receipt)
for path, digest in re.findall(r'Production source (js/[^:]+): \d+ bytes; SHA256 ([0-9a-f]{64})', previous_receipt.read_text()):
    assert hashlib.sha256((ROOT / path).read_bytes()).hexdigest() == digest, path
print('Rust and JS production bytes match the retained complete protocol validation; no unchanged compiler suites repeated.')
print('Allocator-gated owned preview uses the ordinary matching/mutation code without observer callbacks; no probe rows reach private or published storage.')
print('FIFO reservations consume only after write-lock admission; exact explicit ensures retire only their address. Rollback clears ownership but can retain globally reserved gaps.')
print('Cost: a detached visible-row/name snapshot, O(N log N) sorted construction, a second query execution and O(K) reservation state; expected O(1) reservation consumption/retirement. Raw sessions allocate no reservation queue until used.')
print('Compatible defaults retain recycling prediction and custom creator behavior. Direct ensure avoids walking intervening addresses; a four-create guard bounds the full-u64 regression.')
print('Checked causal query admission/adapters, C# shared query vectors, physical query interchange, whole-project 100% and every other unmet R1-R12 criterion remain unfinished. PR139 stays draft/unmerged.')
FILES.update((ROOT / 'csharp/Foundation.Data.Doublets.Cli.Library').rglob('*.cs'))
FILES = {p for p in FILES if 'obj' not in p.parts and 'bin' not in p.parts}
for name in ['QueryAllocationTests.cs', 'EngineQueryReservationTests.cs', 'QueryPreviewContracts.cs']:
    FILES.add(ROOT / 'csharp/Foundation.Data.Doublets.Cli.Tests' / name)
FILES.add(Path(__file__).resolve())
for pattern in ['csharp-query-allocation/*.cs', 'csharp-query-allocation/*.csproj',
                '*csharp-query*.sh', '*csharp-generic-query*.sh']:
    FILES.update(BASE.glob(pattern))
for path in sorted(FILES):
    data = path.read_bytes()
    print(f'{path.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
