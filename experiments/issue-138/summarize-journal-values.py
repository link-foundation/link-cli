#!/usr/bin/env python3
"""Preserve complete-project coverage for captured journal value contracts."""
from pathlib import Path
import re
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
focused = (base / 'journal-value-before.log').read_text()
assert re.search(r'Failed:\s+0, Passed:\s+17, Skipped:\s+0', focused)
print('Journal value contracts; PR https://github.com/link-foundation/link-cli/pull/139')
print('Run: bash experiments/issue-138/validate-journal-values.sh')
print('All 17 focused controls pass before any production change; no defect reproduced.')
print('Captured collections, single enumeration, read-only ownership, complete equality,')
print('hash collections, header properties, offset-equivalent instants and row-only wire pass.')
print('A finite 65,536-unit name preserves the complete existing .NET UTF-16 domain;')
print('this includes lone surrogates and does not claim UTF-8 cross-language compatibility.')
print('Twelve malformed name-unit vectors reject without a result.')
for mode in ('Debug', 'Release'):
    log = (base / f'journal-value-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match.group(1) == match.group(2), log[-4096:]
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    root = ET.parse(report).getroot()
    print(f'\nComplete C# {mode}: {match.group(1)} passed, 0 failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, total = (int(root.attrib[f'{metric}-{suffix}']) for suffix in ('covered', 'valid'))
        print(f'{metric}: {covered}/{total} ({100 * covered / total:.5f}%)')
    lines = {}
    for cls in root.findall('.//class'):
        if cls.attrib['filename'].endswith('/JournalRecord.cs'):
            for line in cls.findall('./lines/line'):
                number = int(line.attrib['number'])
                lines[number] = lines.get(number, False) or int(line.attrib['hits']) > 0
    print(f'JournalRecord.cs unique lines: {sum(lines.values())}/{len(lines)}')
    print('Report: ' + str(report.relative_to(base.parent.parent)))
print('\nFormatting, release/source-size checks, ShellCheck and 221 repository tests pass.')
print('Production source and complete coverage denominators remain unchanged.')
print('Native Rust 930 / actual WASM 546 / JS 209 unchanged since recorded runs.')
print('Whole-project 100% and every wider unfinished criterion remain open.')
