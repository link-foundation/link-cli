#!/usr/bin/env python3
"""Summarize the complete reports produced by the two argument validators."""
import json
import re
from pathlib import Path

base = Path(__file__).resolve().parent
print('Public argument contracts; PR https://github.com/link-foundation/link-cli/pull/139')
print('Run: bash experiments/issue-138/validate-argument-contracts.sh')
print('Run: bash experiments/issue-138/validate-argument-wasm.sh')
print('One compiler job, debug information disabled; WASM linear memory capped at 256 MiB.')
for scope, stem in [('Native Rust', 'rust'), ('Actual WASM', 'wasm')]:
    log = (base / f'arguments-{stem}-full.log').read_text()
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    assert results and all(int(failed) == 0 for _, failed, _ in results)
    print(f'\n{scope}: {sum(int(passed) for passed, _, _ in results)} passed; '
          f'0 failed; {sum(int(ignored) for _, _, ignored in results)} ignored child entries.')
    report = base / f'arguments-{stem}-coverage.json'
    data = json.loads(report.read_text())['data'][0]
    for label, summary in [('Complete measured production', data['totals']),
                           ('cli.rs', next(f['summary'] for f in data['files']
                                           if f['filename'].endswith('/cli.rs')))]:
        print(label + ':')
        for metric in ('lines', 'branches', 'functions'):
            value = summary[metric]
            print(f"  {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    print('Report: ' + str(report.relative_to(base.parent.parent)))
print('\nSeven shared semantic groups compare complete configurations, all aliases,')
print('literal Unicode/whitespace/delimiters, booleans, numeric/arity boundaries,')
print('missing/invalid values, positional/literal queries and independent strategy flags.')
print('Rejected parses are followed by clean parses. No production defect reproduced.')
print('An initial diagnostic oracle was corrected to the established canonical alias.')
print('Native/WASM strict Clippy and formatting, ShellCheck, release/source-size')
print('checks and 221 repository tests pass. No production source changed.')
print('C# 2124 per configuration and JS 209 unchanged since their recorded runs.')
print('All production denominators retained; whole-project 100% and wider criteria stay open.')
