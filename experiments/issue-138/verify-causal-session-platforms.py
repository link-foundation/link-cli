#!/usr/bin/env python3
"""Verify all six causal components in exact-head Linux/macOS/Windows archives.

Usage: verify-causal-session-platforms.py HEAD RUN PREFIX
Read PREFIX-artifacts.json (paginated GitHub API, optionally --slurp),
PREFIX-jobs.json (gh run view --json headSha,jobs), and immutable archives at
ci-logs/PREFIX-ARTIFACT_ID.zip. Print a reviewable receipt only after checking
every job, source identity, archive digest and raw compiled branch record.
"""
import hashlib
import json
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
import zipfile

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]
head, run_text, prefix = sys.argv[1:]
run = int(run_text)
assert len(head) == 40 and all(c in '0123456789abcdef' for c in head)
metadata_path = BASE / f'{prefix}-artifacts.json'
jobs_path = BASE / f'{prefix}-jobs.json'
pages = json.loads(metadata_path.read_text())
if isinstance(pages, dict):
    pages = [pages]
artifacts = [artifact for page in pages for artifact in page['artifacts']]
assert len(artifacts) == pages[0]['total_count'], 'Incomplete artifact pagination'
jobs = json.loads(jobs_path.read_text())
assert jobs['headSha'] == head
components = {'CausalVersion.cs', 'CausalReplica.cs', 'CausalMerkle.cs',
              'CausalBatch.cs', 'CausalProjection.cs', 'CausalSession.cs'}
reports = [metadata_path, jobs_path]
lines = ['PR: https://github.com/link-foundation/link-cli/pull/139',
         f'Head: {head}',
         f'C# run: https://github.com/link-foundation/link-cli/actions/runs/{run}']
for platform, artifact_name, job_name in [('linux', 'csharp-coverage', 'Test (ubuntu-24.04)'),
                                          ('macos', 'csharp-coverage-macos-26', 'Test (macos-26)'),
                                          ('windows', 'csharp-coverage-windows-2025', 'Test (windows-2025)')]:
    record, = [item for item in artifacts if item['name'] == artifact_name]
    assert record['workflow_run']['id'] == run
    assert record['workflow_run']['head_sha'] == head
    assert record['workflow_run']['head_branch'] == 'issue-138-adc2aa56c4cb'
    job, = [item for item in jobs['jobs'] if item['name'] == job_name]
    assert job['conclusion'] == 'success'
    archive_path = BASE / f'ci-logs/{prefix}-{record["id"]}.zip'
    archive_bytes = archive_path.read_bytes()
    assert len(archive_bytes) == record['size_in_bytes']
    assert record['digest'] == 'sha256:' + hashlib.sha256(archive_bytes).hexdigest()
    reports.append(archive_path)
    lines += ['', f'{platform}: https://github.com/link-foundation/link-cli/actions/runs/{run}/job/{job["databaseId"]}',
              f'Successful job: {job["startedAt"]} through {job["completedAt"]}.']
    with zipfile.ZipFile(archive_path) as archive:
        json_name, = [name for name in archive.namelist() if name.endswith('coverage.json')]
        xml_name, = [name for name in archive.namelist() if name.endswith('coverage.cobertura.xml')]
        raw_json, raw_xml = archive.read(json_name), archive.read(xml_name)
    for name, data in [(json_name, raw_json), (xml_name, raw_xml)]:
        lines.append(f'{name}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
    report = ET.fromstring(raw_xml)
    for metric in ('lines', 'branches'):
        covered, total = [int(report.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        lines.append(f'Complete {metric}: {covered}/{total} ({100 * covered / total:.5f}%).')
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
    assert set(files) == components, (platform, set(files))
    for name, branches in sorted(files.items()):
        assert branches and all(branch['Hits'] > 0 for branch in branches), (platform, name)
        lines.append(f'{name}: {len(branches)}/{len(branches)} compiled branches, including methods without source lines.')
lines += ['', 'All three independent platform archives retain raw JSON and Cobertura XML.',
          'Every compiled branch in all six causal components is visited.',
          'Whole-project coverage and every wider unfinished R1-R12 criterion remain active.',
          'Later commits require fresh exact-head CI; PR139 remains draft and unmerged.']
for report in reports:
    data = report.read_bytes()
    lines.append(f'{report.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
print('\n'.join(lines))
