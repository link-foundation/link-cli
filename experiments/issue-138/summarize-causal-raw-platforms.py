#!/usr/bin/env python3
"""Verify immutable raw Coverlet archives from the published pre-session head."""
import hashlib
import json
from pathlib import Path
import re
import zipfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'experiments/issue-138'
HEAD = '86378e89854498ee20f309f8cff599e54d9749a1'
RUN = 40568997232
metadata_path = BASE / 'causal-raw-platform-artifacts.json'
metadata = json.loads(metadata_path.read_text())
jobs = json.loads((BASE / 'causal-raw-csharp-jobs.json').read_text())
assert jobs['headSha'] == HEAD
print('PR: https://github.com/link-foundation/link-cli/pull/139')
print(f'Published pre-session head: {HEAD}')
print(f'C# run: https://github.com/link-foundation/link-cli/actions/runs/{RUN}')
reports = [metadata_path, BASE / 'causal-raw-csharp-jobs.json']
components = {'CausalBatch.cs', 'CausalMerkle.cs', 'CausalProjection.cs', 'CausalReplica.cs', 'CausalVersion.cs'}
for platform, artifact, job in [('linux', 12764408073, 122420909140),
                                ('macos', 12765037640, 122420909164),
                                ('windows', 12765338438, 122420909090)]:
    record, = [r for r in metadata['artifacts'] if r['id'] == artifact]
    assert record['workflow_run']['id'] == RUN
    assert record['workflow_run']['head_sha'] == HEAD
    assert record['workflow_run']['head_branch'] == 'issue-138-adc2aa56c4cb'
    status, = [j for j in jobs['jobs'] if j['databaseId'] == job]
    assert status['conclusion'] == 'success'
    path = BASE / f'ci-logs/causal-raw-csharp-{platform}-{artifact}.zip'
    raw = path.read_bytes()
    assert len(raw) == record['size_in_bytes']
    assert record['digest'] == 'sha256:' + hashlib.sha256(raw).hexdigest()
    reports.append(path)
    print(f'\n{platform}: https://github.com/link-foundation/link-cli/actions/runs/{RUN}/job/{job}')
    print(f"Successful job: {status['startedAt']} through {status['completedAt']}.")
    with zipfile.ZipFile(path) as archive:
        json_name, = [name for name in archive.namelist() if name.endswith('coverage.json')]
        xml_name, = [name for name in archive.namelist() if name.endswith('coverage.cobertura.xml')]
        raw_json, raw_xml = archive.read(json_name), archive.read(xml_name)
    for name, value in [(json_name, raw_json), (xml_name, raw_xml)]:
        print(f'{name}: {len(value)} bytes; SHA256 {hashlib.sha256(value).hexdigest()}.')
    report = ET.fromstring(raw_xml)
    for metric in ('lines', 'branches'):
        covered, total = [int(report.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{total} ({100 * covered / total:.5f}%).')
    files = {}
    for documents in json.loads(raw_json).values():
        for filename, classes in documents.items():
            name = Path(filename.replace('\\', '/')).name
            if name not in components:
                continue
            branches = files.setdefault(name, [])
            for methods in classes.values():
                for method in methods.values():
                    branches.extend(method['Branches'])
    assert set(files) == components
    for name, branches in sorted(files.items()):
        assert branches and all(branch['Hits'] > 0 for branch in branches)
        print(f'{name}: {len(branches)}/{len(branches)} compiled branches, including methods without source lines.')
print('\nAll three independent archives retain raw JSON and Cobertura XML. Whole-project coverage remains below 100%.')
before = (BASE / 'causal-session-raw-windows-before.log').read_text()
assert 'FAILED (failures=2)' in before and 'AssertionError: []' in before
after = (BASE / 'causal-session-raw-verifier.log').read_text()
assert re.search(r'Ran 5 tests', after) and after.rstrip().endswith('OK')
print('Windows-path regression: two verifier contracts fail before slash normalization; all five pass afterward, including new-session admission.')
print('The original five causal component results are immutable pre-session evidence. New session validation and every wider R1-R12 criterion remain separately tracked; PR139 stays draft.')
reports.extend([BASE / 'causal-session-raw-windows-before.log', BASE / 'causal-session-raw-verifier.log',
                BASE / 'check-causal-raw-coverage.py', BASE / 'test-causal-raw-coverage.py'])
for path in reports:
    raw = path.read_bytes()
    print(f'{path.relative_to(ROOT)}: {len(raw)} bytes; SHA256 {hashlib.sha256(raw).hexdigest()}.')
