#!/usr/bin/env python3
"""Keep exact, complete C# coverage after the bounded mapped-growth regression."""
from pathlib import Path
import re
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
before = (base / 'mapped-growth-before-corrected.log').read_text()
assert 'Failed:     1, Passed:     1' in before and 'System.ObjectDisposedException' in before
focused = (base / 'mapped-growth-final-focused.log').read_text()
assert 'Failed:     0, Passed:     2' in focused
print('Owned mapped growth; PR https://github.com/link-foundation/link-cli/pull/139')
print('Run: bash experiments/issue-138/validate-mapped-growth.sh')
print('Before: 1 failed / 1 passing control; see mapped-growth-before.txt.')
print('After: 2 focused tests passed; actual Linux quota child, 8192-byte file quota,')
print('128 MiB child GC heap, finite deadline and three-page maximum requested file.')
for mode in ('Debug', 'Release'):
    log = (base / f'mapped-growth-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match.group(1) == match.group(2), log[-4096:]
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    root = ET.parse(report).getroot()
    print(f'\nComplete C# {mode}: {match.group(1)} passed, 0 failed/skipped.')
    for metric in ('lines', 'branches'):
        covered = int(root.attrib[f'{metric}-covered'])
        total = int(root.attrib[f'{metric}-valid'])
        print(f'{metric}: {covered}/{total} ({100 * covered / total:.5f}%)')
    lines = {}
    for cls in root.findall('.//class'):
        if cls.attrib['filename'].endswith('/DurableMappedMemory.cs'):
            for line in cls.findall('./lines/line'):
                number = int(line.attrib['number'])
                lines[number] = lines.get(number, False) or int(line.attrib['hits']) > 0
    print(f'DurableMappedMemory.cs unique lines: {sum(lines.values())}/{len(lines)}')
    print('Report: ' + str(report.relative_to(base.parent.parent)))
print('\nRepeated failed growth preserves current reservation, pointer, unflushed bytes,')
print('flush and file ownership. Failed initialization releases its newly opened handle.')
print('Following growth/writes, shrink/regrowth, exact used-length disposal and reopening pass.')
print('Formatting, release/source-size checks, ShellCheck and 221 repository tests pass.')
print('Native Rust 930 / actual WASM 546 / JS 209 unchanged since their recorded runs.')
print('No power-loss or legacy physical-repair certification claimed.')
print('Whole-project 100% and every wider unfinished acceptance criterion remain open.')
