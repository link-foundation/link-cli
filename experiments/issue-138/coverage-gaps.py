"""Print bounded LLVM source/branch gaps without truncating a large report."""
import json
import sys
from pathlib import Path
report = json.load(open(sys.argv[1]))['data'][0]
for file in report['files']:
    if sys.argv[2] not in file['filename']:
        continue
    source = Path(file['filename']).read_text().splitlines()
    print('\n' + file['filename'])
    branches = {}
    for branch in file['branches']:
        key = tuple(branch[:4])
        counts = branches.setdefault(key, [0, 0])
        counts[0] += branch[4]
        counts[1] += branch[5]
    for (line, col, end_line, end_col), counts in sorted(branches.items()):
        if not all(counts):
            print(f'branch {line}:{col} taken={counts}: {source[line-1].strip()}')
    lines = {}
    for line, col, count, has_count, entry, gap in file['segments']:
        if has_count and not gap:
            lines.setdefault(line, []).append(count)
    for line, counts in lines.items():
        if not any(counts):
            print(f'line {line}: {source[line-1].strip()}')
