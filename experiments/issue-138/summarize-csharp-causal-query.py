#!/usr/bin/env python3
"""Record complete checked-query evidence without hiding uncovered methods."""
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'experiments/issue-138'
PREFIX = 'csharp-causal-query'
FILES = set()


def read(name):
    path = BASE / name
    FILES.add(path)
    return path.read_text()


print('PR: https://github.com/link-foundation/link-cli/pull/139')
print('Reproduce: bash experiments/issue-138/validate-csharp-causal-query.sh')
driver = read(f'{PREFIX}-validation.log')
assert all(text in driver for text in ['Debug PASS ', 'Release PASS ', 'ALL PASS '])
print(driver.strip())
assert 'line 8: uery.sh: command not found' in driver
print('Wrapper audit: the outer shell returned 127 after ALL PASS because its output-log name was edited while Bash was reading the file. All test/coverage/gate subprocesses had already completed successfully. The corrected stable wrapper passes bash -n and ShellCheck; the finite vector probe was rerun into its distinct log. No full-suite rerun is claimed for that naming-only correction.')
wrapper = read(f'{PREFIX}-wrapper-check.log')
assert 'PASS stable wrapper syntax and ShellCheck' in wrapper
assert "does not contain a definition for 'QueryLinks'" in read(f'{PREFIX}-before.log')
assert "does not contain a definition for 'QueryLinks'" in read(f'{PREFIX}-unit-before.log')
print('Before: standalone shared vectors and permanent contracts cannot compile: CausalSession.QueryLinks is missing.')
draft = read(f'{PREFIX}-focused-draft.log')
assert re.search(r'Failed:\s+6,\s+Passed:\s+20,\s+Skipped:\s+0,\s+Total:\s+26', draft)
print('First executable draft: six failures / twenty passing controls. Full-range constants must keep Any outside u64; seed edits must be sorted; pinned CreateAndUpdate invokes the unsupported upstream UInt128 list initializer.')
read(f'{PREFIX}-focused-second.log')
read(f'{PREFIX}-focused-third.log')
print('Follow-ups: corrected nullable callback annotations; increased a long-key test from a passing 2 MiB below-bound control to a finite 5 MiB refusal. Production bounds were unchanged.')
vectors = read(f'{PREFIX}-vectors-after.log')
assert 'PASS 16 shared full-width checked C# causal query vectors.' in vectors
print(vectors.strip())
assert 'PASS: shared generic query API' in read(f'{PREFIX}-after.log')
assert '3 passed; 0 failed' in read(f'{PREFIX}-zero-after.log')
print('Selected-address generic and all three zero-binding probes remain passing.')
new_sources = {'CausalQueryLinks.cs', 'QueryAddressScope.cs', 'QueryMatching.cs', 'QueryLimits.cs',
               'AdvancedMixedQueryProcessor.Admission.cs', 'AdvancedMixedQueryProcessor.Creation.cs', 'QueryPreview.cs'}
causal = {'CausalBatch.cs', 'CausalMerkle.cs', 'CausalProjection.cs',
          'CausalReplica.cs', 'CausalSession.cs', 'CausalVersion.cs'}
for mode in ['Debug', 'Release']:
    output = read(f'{PREFIX}-{mode}-tests.log')
    match = re.search(r'Failed:\s+0,\s+Passed:\s+(\d+),\s+Skipped:\s+0,\s+Total:\s+\1', output)
    assert match, mode
    assert int(match[1]) >= 2571
    results = Path(read(f'{PREFIX}-{mode}-results-path.txt').strip())
    raw, = results.glob('*/coverage.json')
    xml, = results.glob('*/coverage.cobertura.xml')
    FILES.update([raw, xml])
    report = ET.fromstring(xml.read_bytes())
    print(f'{mode}: {match[1]} passed; zero failed/skipped. Complete production Cobertura:')
    for metric in ['lines', 'branches']:
        covered, total = int(report.attrib[f'{metric}-covered']), int(report.attrib[f'{metric}-valid'])
        print(f'  {metric}: {covered}/{total} ({100 * covered / total:.5f}%).')
    old_results = Path(read(f'csharp-query-reservations-{mode}-results-path.txt').strip())
    old, = old_results.glob('*/coverage.cobertura.xml')
    FILES.add(old)
    previous = {c.attrib['filename'] for c in ET.fromstring(old.read_bytes()).iter('class')}
    current = {c.attrib['filename'] for c in report.iter('class')}
    assert previous <= current, sorted(previous - current)
    print(f'  Every previous production file retained: {len(previous)} previous, {len(current)} current.')
    seen = set()
    for documents in json.loads(raw.read_bytes()).values():
        for filename, classes in documents.items():
            name = Path(filename.replace('\\', '/')).name
            if name not in new_sources | causal:
                continue
            seen.add(name)
            lines, branches = {}, []
            for methods in classes.values():
                for evidence in methods.values():
                    for line, hits in evidence['Lines'].items():
                        lines[line] = lines.get(line, 0) + hits
                    branches.extend(evidence['Branches'])
            missed = [branch for branch in branches if not branch['Hits']]
            print(f'  {name}: {sum(bool(v) for v in lines.values())}/{len(lines)} source lines; {len(branches)-len(missed)}/{len(branches)} raw compiled branches.')
            if name in causal:
                assert lines and all(lines.values()), name
                assert not missed, name
            for branch in missed:
                print(f'    Uncovered compiled branch: {branch}')
    assert seen == new_sources | causal, seen
    raw_result = read(f'{PREFIX}-{mode}-causal-coverage.log')
    assert 'PASS every compiled causal branch' in raw_result
    print(raw_result.strip())
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
print('Rust and JS production bytes match the retained complete protocol validation; unchanged compiler suites were not repeated.')
print('The checked UInt128 adapter preserves every stored u64 identity, including MAX, separately from Any. Conversion and owner-lifetime guards precede private reads/writes; callbacks and escaped previews own detached parts.')
print('Input, depth, nodes, numeric, matching, bindings and decoded named reply budgets run before real query edits or external callbacks. A retained high tombstone can be resurrected without advancing a lower fresh reservation.')
print('Shared vectors verify private/public separation, following cursor, exact batch edits and original-token publication retries. Unknown publication replies retain preparation and reject later queries.')
print('Costs: O(N log N) detached sorted row/name snapshot, repeated query execution, O(K) lazy FIFO reservations and bounded matching/binding/change maps. Direct Count/Each currently scan and own matched rows; large stores may refuse broad reads under the finite profile.')
print('C# standard selected-address protocols/lossless envelope and physical query interchange remain unfinished. Whole-project 100% and every wider unmet R1-R12 criterion remain unfinished; PR139 stays draft/unmerged.')
FILES.update((ROOT / 'csharp/Foundation.Data.Doublets.Cli.Library').rglob('*.cs'))
FILES = {p for p in FILES if 'obj' not in p.parts and 'bin' not in p.parts}
FILES.update((ROOT / 'csharp/Foundation.Data.Doublets.Cli.Tests').glob('CausalQuery*.cs'))
FILES.add(ROOT / 'csharp/Foundation.Data.Doublets.Cli.Tests/Foundation.Data.Doublets.Cli.Tests.csproj')
FILES.add(ROOT / 'docs/protocol/causal-query-v1.json')
FILES.add(Path(__file__).resolve())
for pattern in ['csharp-causal-query/*.cs', 'csharp-causal-query/*.csproj', '*csharp-query*.sh', '*csharp-generic-query*.sh', '*csharp-causal-query*.sh']:
    FILES.update(BASE.glob(pattern))
for path in sorted(FILES):
    data = path.read_bytes()
    print(f'{path.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
