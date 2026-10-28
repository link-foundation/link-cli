#!/usr/bin/env python3
"""Check every author guard line and raw branch; retain complete report identity."""
import argparse
import hashlib
import json
import xml.etree.ElementTree as ET
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('report', type=Path)
args = parser.parse_args()
raw = args.report.read_bytes()
print(f'Report: {args.report}; bytes {len(raw)}; SHA256 {hashlib.sha256(raw).hexdigest()}')
complete = ET.parse(args.report.with_name('coverage.cobertura.xml')).getroot()
for metric in ['lines', 'branches']:
    print(f'Complete production {metric}: {complete.attrib[metric+"-covered"]}/'
          f'{complete.attrib[metric+"-valid"]}; focused execution is not full-project coverage')
lines, branches = {}, []
found = False
for documents in json.loads(raw).values():
    for filename, classes in documents.items():
        if Path(filename.replace('\\', '/')).name != 'AuthorFence.cs':
            continue
        found = True
        for methods in classes.values():
            for method, evidence in methods.items():
                for number, hits in evidence['Lines'].items():
                    lines[number] = lines.get(number, 0) + hits
                for branch in evidence['Branches']:
                    branches.append(branch)
                    if branch['Hits'] == 0:
                        print(f'UNCOVERED {method}; {branch}')
assert found and lines and branches, 'Missing author fence instrumentation'
uncovered_lines = sorted(int(n) for n, hits in lines.items() if hits == 0)
uncovered_branches = sum(b['Hits'] == 0 for b in branches)
print(f'AuthorFence.cs: {len(lines)-len(uncovered_lines)}/{len(lines)} lines, '
      f'{len(branches)-uncovered_branches}/{len(branches)} raw compiled branches')
print(f'Uncovered source lines: {uncovered_lines}')
assert not uncovered_lines and not uncovered_branches, 'Uncovered author guard contracts'
print('PASS all author guard lines and raw branches; complete project coverage is separate')
