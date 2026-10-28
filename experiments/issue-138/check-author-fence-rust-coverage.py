#!/usr/bin/env python3
"""Reject uncovered author-guard counters without changing report denominators."""
import argparse
import hashlib
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('report', type=Path)
args = parser.parse_args()
raw = args.report.read_bytes()
print(f'Report: {args.report}; bytes {len(raw)}; SHA256 {hashlib.sha256(raw).hexdigest()}')
print('Report scope is retained unchanged; focused execution is not a full-suite coverage claim')
target = '/src/engine/author_fence.rs'
selected = []
if args.report.suffix == '.json':
    report = json.loads(raw)
    assert report['type'] == 'llvm.coverage.json.export', 'Not an official LLVM export'
    metrics = ['lines', 'branches', 'functions', 'regions', 'instantiations']
    for group in report['data']:
        print('Complete instrumented production totals: ' + json.dumps(group['totals'], sort_keys=True))
        for source in group['files']:
            if source['filename'].replace('\\', '/').endswith(target):
                selected.append(source['summary'])
else:
    # Retained older CI artifacts contain LCOV only. These counters are export
    # counts, not replacements for official LLVM file/instantiation summaries.
    metrics = ['lines', 'branches', 'functions']
    names = {'LF': ('lines', 'count'), 'LH': ('lines', 'covered'),
             'BRF': ('branches', 'count'), 'BRH': ('branches', 'covered'),
             'FNF': ('functions', 'count'), 'FNH': ('functions', 'covered')}
    totals = {metric: {'count': 0, 'covered': 0} for metric in metrics}
    source, summary = '', {}
    for line in raw.decode().splitlines():
        if line.startswith('SF:'):
            source = line[3:].replace('\\', '/')
        elif line == 'end_of_record':
            for metric, counters in summary.items():
                for key, value in counters.items():
                    totals[metric][key] += value
            if source.endswith(target):
                selected.append(summary)
            source, summary = '', {}
        elif line.partition(':')[0] in names:
            name, value = line.split(':', 1)
            metric, key = names[name]
            summary.setdefault(metric, {})[key] = int(value)
    print('Complete LCOV export totals: ' + json.dumps(totals, sort_keys=True))
    print('LCOV omits official LLVM regions/instantiations; selected counts are export evidence only')
assert len(selected) == 1, f'Expected one author-guard source, found {len(selected)}'
missing = []
for metric in metrics:
    counters = selected[0][metric]
    count, covered = counters['count'], counters['covered']
    print(f'Author guard {metric}: {covered}/{count}')
    assert count > 0, f'Missing {metric} instrumentation'
    if covered != count and metric != 'instantiations':
        missing.append(metric)
    if metric == 'instantiations' and covered != count:
        print('UNFINISHED: unexecuted LLVM instantiation entries remain in the report and acceptance inventory')
assert not missing, f'Uncovered author guard counters: {missing}'
print('PASS required author-guard counters: ' + ', '.join(m for m in metrics if m != 'instantiations'))
print('Instantiation and full-project acceptance gaps remain separate and unfinished')
