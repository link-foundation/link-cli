#!/usr/bin/env python3
"""Print official complete/file counters without excluding generic entries."""
import hashlib
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
with path.open('rb') as stream:
    digest = hashlib.file_digest(stream, 'sha256').hexdigest()
report = json.loads(path.read_text())
print(f'Report: {path}; SHA256 {digest}')
assert len(report['data']) == 1
data = report['data'][0]
print('Complete instrumented production totals:', json.dumps(data['totals'], sort_keys=True))
files = [f for f in data['files']
         if f['filename'].endswith('/rust/src/storage/doublets_storage.rs')]
assert len(files) == 1
print('DoubletsStorage official file summary:', json.dumps(files[0]['summary'], sort_keys=True))
print('Focused execution does not establish complete-project or filesystem coverage.')
