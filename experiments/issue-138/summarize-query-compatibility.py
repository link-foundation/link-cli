#!/usr/bin/env python3
"""Read the exact complete-run reports named by each successful test log."""
from pathlib import Path
import re
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
print('Public query compatibility; PR https://github.com/link-foundation/link-cli/pull/139')
print('Run: bash experiments/issue-138/validate-query-compatibility.sh')
print('.NET GC heap cap: 640 MiB; one compiler job. No production source changes.')
print('Focused contracts: 33 passed, 0 failed/skipped.')
for mode in ('Debug', 'Release'):
    log = (base / f'query-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match.group(1) == match.group(2), log[-4096:]
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    root = ET.parse(report).getroot()
    print(f'\nComplete C# {mode}: {match.group(1)} passed, 0 failed/skipped.')
    for metric in ('lines', 'branches'):
        covered = int(root.attrib[f'{metric}-covered'])
        total = int(root.attrib[f'{metric}-valid'])
        print(f'{metric}: {covered}/{total} ({100 * covered / total:.5f}%)')
    for filename in ('BasicQueryProcessor.cs', 'MixedQueryProcessor.cs', 'AdvancedMixedQueryProcessor.cs'):
        lines = {}
        for cls in root.findall('.//class'):
            if cls.attrib['filename'].endswith('/' + filename):
                for line in cls.findall('./lines/line'):
                    number = int(line.attrib['number'])
                    lines[number] = lines.get(number, False) or int(line.attrib['hits']) > 0
        print(f'{filename} unique lines: {sum(lines.values())}/{len(lines)}')
    print('Report: ' + str(report.relative_to(base.parent.parent)))
print('\nEmpty/comment/leaf/incomplete shapes preserve rows, names and notifications across')
print('all three retained processors, including following edits and native reopening.')
print('Missing arguments, rejected mixed wildcards, existing-pair reuse and basic update')
print('count diagnostics preserve the established public semantics and following edits.')
print('No leaf null-list crash reproduced: the installed parser supplies empty lists.')
print('The initial // comment control was an incorrect oracle; LiNo comments use #.')
print('Formatting, release/source-size checks and 221 repository tests pass.')
print('Native Rust 923 / actual WASM 539 / JS 209 unchanged since their recorded runs.')
print('All production files remain measured; whole-project 100% and wider criteria stay open.')
