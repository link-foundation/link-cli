#!/usr/bin/env python3
"""Validate exact-head workflow completion and creation dates before recording it."""
import json
import subprocess
import sys
from datetime import datetime
from pathlib import Path

commit, title, source, destination = sys.argv[1:]
sha = subprocess.check_output(['git', 'rev-parse', commit], text=True).strip()
created = datetime.fromisoformat(subprocess.check_output(
    ['git', 'show', '-s', '--format=%cI', sha], text=True).strip())
runs = [run for run in json.loads(Path(source).read_text()) if run['headSha'] == sha]
expected = {'Broken Link Checker', 'C# CI/CD Pipeline', 'Dependencies', 'Docs',
            'JS client CI', 'Rust CI/CD Pipeline', 'Security', 'WebAssembly CI', 'Workflows'}
assert {run['workflowName'] for run in runs} == expected and len(runs) == 9, runs
assert all(run['status'] == 'completed' and run['conclusion'] == 'success'
           and datetime.fromisoformat(run['createdAt'].replace('Z', '+00:00')) >= created
           for run in runs), runs
lines = [title + ': exact-head GitHub Actions evidence',
         'PR: https://github.com/link-foundation/link-cli/pull/139', 'Commit: ' + sha,
         'Commit created: ' + created.isoformat(),
         'All nine workflows completed successfully after this commit.', '']
for run in sorted(runs, key=lambda item: item['workflowName']):
    lines.append(f"{run['workflowName']}: success; created {run['createdAt']}; "
                 f"https://github.com/link-foundation/link-cli/actions/runs/{run['databaseId']}")
lines += ['', 'Native Linux/macOS/Windows matrices and actual WASM pass. Later commits need fresh CI.',
          'Whole-project coverage and every wider unfinished requirement remain active.']
Path(destination).write_text('\n'.join(lines) + '\n')
