#!/usr/bin/env python3
"""Snapshot CI's Rust source inputs without ignored historical checkouts."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
base = root / 'experiments/issue-138'
parent = base / 'query-source-size/target'
parent.mkdir(parents=True, exist_ok=True)
destination = Path(tempfile.mkdtemp(prefix='checkout-', dir=parent))
files = subprocess.check_output(
    ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=root
).decode().split('\0')
records = []
for filename in sorted(set(files)):
    if not filename.endswith('.rs'):
        continue
    source = root / filename
    if not source.exists():
        continue
    assert not source.is_symlink(), filename
    copied = destination / filename
    copied.parent.mkdir(parents=True, exist_ok=True)
    data = source.read_bytes()
    copied.write_bytes(data)
    assert copied.read_bytes() == data, filename
    records.append({'path': filename, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
assert 'rust/src/query_processor.rs' in {record['path'] for record in records}
assert 'rust/src/engine/query_reservation.rs' in {record['path'] for record in records}
prefix = root / os.environ.get('VALIDATION_PREFIX', 'experiments/issue-138/query-reservation')
Path(f'{prefix}-rust-source-snapshot.json').write_text(json.dumps(
    {'destination': str(destination), 'files': records}, indent=2) + '\n')
print(destination)
