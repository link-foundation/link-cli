#!/usr/bin/env python3
"""Download one exact-head native/WASM artifact and verify GitHub's digest."""
import argparse
import hashlib
import json
import re
import subprocess
import zipfile
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('run', type=int)
parser.add_argument('sha')
parser.add_argument('name', choices=['rust-coverage', 'wasm-coverage'])
args = parser.parse_args()
assert re.fullmatch('[0-9a-f]{40}', args.sha)
repository = 'link-foundation/link-cli'
directory = Path(__file__).resolve().parent / f'coverage-{args.run}-{args.name}'
directory.mkdir(exist_ok=True)


def saved_json(filename, command):
    path = directory / filename
    with path.open('wb') as output:
        subprocess.run(['gh', *command], stdout=output, check=True)
    return json.loads(path.read_text())


run = saved_json('run.json', ['run', 'view', str(args.run), '--repo', repository,
                             '--json', 'headSha,status,conclusion,createdAt'])
assert run['headSha'] == args.sha
pages = saved_json('artifacts.json', ['api', f'repos/{repository}/actions/runs/{args.run}/artifacts?per_page=100',
                                     '--paginate', '--slurp'])
artifact, = [a for page in pages for a in page['artifacts'] if a['name'] == args.name]
assert artifact['workflow_run']['head_sha'] == args.sha and not artifact['expired']
assert 0 < artifact['size_in_bytes'] < 32 * 1024 * 1024
archive = directory / 'artifact.zip'
with archive.open('wb') as output:
    subprocess.run(['gh', 'api', f"repos/{repository}/actions/artifacts/{artifact['id']}/zip"], stdout=output, check=True)
assert archive.stat().st_size == artifact['size_in_bytes']
with archive.open('rb') as source:
    digest = hashlib.file_digest(source, 'sha256').hexdigest()
assert artifact['digest'] == 'sha256:' + digest
print(f"Run {args.run}; head {args.sha}; created {run['createdAt']}; {run['status']}/{run['conclusion']}")
print(f"Verified artifact {artifact['id']}; {archive.stat().st_size} bytes; SHA256 {digest}")
allowed = {'coverage.lcov', 'coverage-summary.json', 'wasm-coverage.json'}
with zipfile.ZipFile(archive) as zipped:
    assert sum(info.file_size for info in zipped.infolist()) < 64 * 1024 * 1024
    reports = [name for name in zipped.namelist() if Path(name).name in allowed]
    assert reports and len({Path(name).name for name in reports}) == len(reports)
    for name in reports:
        raw = zipped.read(name)
        destination = directory / Path(name).name
        destination.write_bytes(raw)
        print(f'Preserved {destination}; {len(raw)} bytes; SHA256 {hashlib.sha256(raw).hexdigest()}')
print('A retained artifact does not establish that an in-progress workflow has passed')
