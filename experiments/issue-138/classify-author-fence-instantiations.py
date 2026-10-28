#!/usr/bin/env python3
"""Identify zero-count generic placeholders without dropping LLVM counters."""
import argparse
import hashlib
import json
import re
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('report', type=Path)
args = parser.parse_args()
raw = args.report.read_bytes()
print(f'Full official LLVM export: {args.report}; SHA256 {hashlib.sha256(raw).hexdigest()}')
groups = json.loads(raw)['data']
functions = [f for group in groups for f in group['functions']
             if any(name.endswith('/src/engine/author_fence.rs') for name in f['filenames'])]
missing = [f for f in functions if f['count'] == 0]
assert functions and missing, 'Expected full instrumentation and zero-count entries'
for function in missing:
    print('Zero-count generic placeholder:', function['name'])
    assert re.search(r'(?:AuthorFence|GuardedJournalStore)pE', function['name']), function['name']
for function in functions:
    if function['count'] > 0:
        assert all(region[4] > 0 for region in function['regions']), function['name']
print(f'{len(functions)-len(missing)} executed entries; {len(missing)} zero-count generic placeholders')
print('All executed author-guard entries cover their recorded regions; raw totals remain unchanged')
