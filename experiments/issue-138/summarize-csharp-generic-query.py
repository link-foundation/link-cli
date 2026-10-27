#!/usr/bin/env python3
"""Retain unfiltered selected-address query evidence and exact source hashes."""
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'experiments/issue-138'
FILES = set()


def read(name):
    path = BASE / name
    FILES.add(path)
    return path.read_text()


print('PR: https://github.com/link-foundation/link-cli/pull/139')
print('Reproduce: bash experiments/issue-138/validate-csharp-generic-query.sh')
driver = read('csharp-generic-query-validation.log')
assert 'Debug PASS ' in driver and 'Release PASS ' in driver and 'ALL PASS ' in driver
print(driver.strip())
for name, expected in [
    ('csharp-generic-query-before.log', 'error CS0308'),
    ('csharp-generic-query-row-before.log', 'TypeInitializationException'),
    ('csharp-query-zero-before.log', '2 passed; 1 failed'),
    ('csharp-generic-query-unit-before-zero.log', 'Failed:     1'),
]:
    assert expected in read(name), name
    print(f'Before: {name}: missing API or finite regression reproduced.')
print(read('csharp-generic-query-after.log').strip())
zero = read('csharp-query-zero-after.log')
assert '3 passed; 0 failed' in zero
print(zero.strip())

for mode in ['Debug', 'Release']:
    output = read(f'csharp-generic-query-{mode}-tests.log')
    assert re.search(r'Failed:\s+0,\s+Passed:\s+2514,\s+Skipped:\s+0,\s+Total:\s+2514', output)
    results = Path(read(f'csharp-generic-query-{mode}-results-path.txt').strip())
    raw, = results.glob('*/coverage.json')
    xml, = results.glob('*/coverage.cobertura.xml')
    FILES.update([raw, xml])
    report = ET.fromstring(xml.read_bytes())
    print(f'{mode}: 2514 passed; zero failed/skipped. Complete production Cobertura:')
    for metric in ['lines', 'branches']:
        covered, total = int(report.attrib[f'{metric}-covered']), int(report.attrib[f'{metric}-valid'])
        print(f'  {metric}: {covered}/{total} ({100 * covered / total:.5f}%).')
    old, = (BASE / f'causal-session-{mode}-results').glob('*/coverage.cobertura.xml')
    FILES.add(old)
    previous = {c.attrib['filename'] for c in ET.fromstring(old.read_bytes()).iter('class')}
    current = {c.attrib['filename'] for c in report.iter('class')}
    assert previous <= current, sorted(previous - current)
    print(f'  Every previous production file retained: {len(previous)} previous, {len(current)} current.')
    raw_result = read(f'csharp-generic-query-{mode}-causal-coverage.log')
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

assert read('csharp-generic-query-format.log') == ''
for suffix in ['repository', 'source-size', 'changeset', 'shellcheck', 'release-tests', 'whitespace']:
    read(f'csharp-generic-query-{suffix}.log')
assert re.search(r'(?:#|ℹ) pass 221\b', read('csharp-generic-query-repository.log'))
print('Strict full C# format, 221 repository tests, C# source-size/changeset/release-helper, ShellCheck and whitespace gates pass.')

# The complete prior Rust/actual-WASM/JS driver is retained; these C#-only
# changes must not reuse its result for different production bytes.
snapshot = json.loads(read('causal-protocol-rust-source-snapshot.json'))
for item in snapshot['files']:
    assert hashlib.sha256((ROOT / item['path']).read_bytes()).hexdigest() == item['sha256'], item['path']
previous_receipt = ROOT / 'dev/log/issues/138/pulls/139/causal-protocol.txt'
FILES.add(previous_receipt)
for path, digest in re.findall(r'Production source (js/[^:]+): \d+ bytes; SHA256 ([0-9a-f]{64})', previous_receipt.read_text()):
    assert hashlib.sha256((ROOT / path).read_bytes()).hexdigest() == digest, path
print('Rust and JS production bytes match the retained complete protocol validation; no unchanged compiler suites repeated.')
print('Generic matching/mutations preserve the original uint facade; zero remains a concrete bound reference.')
print('Owned read-only standard callback rows avoid pinned upstream UInt128 default-constant reflection; consumer-side upstream list helpers retain that limitation.')
print('Cost: boxed three-part callback rows and existing matching/materialization costs; selected generic code adds instantiations without copying the algorithms.')
print('Monotonic anonymous allocation, checked causal query admission/adapters, C# shared query vectors, physical query interchange, whole-project 100% and every other unmet R1-R12 criterion remain unfinished. PR139 stays draft/unmerged.')
FILES.update((ROOT / 'csharp/Foundation.Data.Doublets.Cli.Library').rglob('*.cs'))
FILES = {p for p in FILES if 'obj' not in p.parts and 'bin' not in p.parts}
FILES.add(ROOT / 'csharp/Foundation.Data.Doublets.Cli.Tests/GenericQueryAddressTests.cs')
FILES.add(Path(__file__).resolve())
FILES.add(BASE / 'validate-csharp-query-gates.sh')
for pattern in ['csharp-generic-query/*.cs', 'csharp-generic-query/*.csproj',
                'csharp-query-zero/*.cs', 'csharp-query-zero/*.csproj',
                '*csharp-generic-query*.sh', 'run-csharp-query-zero.sh']:
    FILES.update(BASE.glob(pattern))
for path in sorted(FILES):
    data = path.read_bytes()
    print(f'{path.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
