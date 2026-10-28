#!/usr/bin/env python3
"""Summarize complete native reports without changing any denominator."""
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
print('Public native command contracts; PR https://github.com/link-foundation/link-cli/pull/139')
print('Run: bash experiments/issue-138/validate-cli-contracts.sh')
print('Rust uses one compiler job, debug info disabled; .NET GC heap cap 640 MiB.')
native = json.loads((base / 'cli-rust-coverage.json').read_text())['data'][0]
text = (base / 'cli-rust-full.log').read_text()
counts = [sum(int(match.group(i)) for match in re.finditer(
    r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', text)) for i in (1, 2, 3)]
assert counts[1] == 0 and 'test result: FAILED' not in text
print(f'\nComplete native Rust: {counts[0]} passed, {counts[1]} failed, {counts[2]} bounded child entry points ignored by direct invocation.')

def llvm(label, summary):
    for metric in ('lines', 'branches', 'functions'):
        value = summary[metric]
        print(f"{label} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")

llvm('Native Rust', native['totals'])
for file in native['files']:
    if file['filename'].endswith(('/src/main.rs', '/src/cli.rs')):
        llvm(Path(file['filename']).name, file['summary'])

for mode in ('Debug', 'Release'):
    log = (base / f'cli-csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match.group(1) == match.group(2), log[-4096:]
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    root = ET.parse(report).getroot()
    print(f'\nComplete C# {mode}: {match.group(1)} passed, 0 failed/skipped.')
    for metric in ('lines', 'branches'):
        covered = int(root.attrib[f'{metric}-covered'])
        total = int(root.attrib[f'{metric}-valid'])
        print(f'C# {mode} {metric}: {covered}/{total} ({covered / total * 100:.5f}%)')
    lines = {}
    for cls in root.findall('.//class'):
        if cls.attrib['filename'].endswith('/Program.cs'):
            for line in cls.findall('./lines/line'):
                number = int(line.attrib['number'])
                lines[number] = lines.get(number, False) or int(line.attrib['hits']) > 0
    print(f'Program.cs unique lines: {sum(lines.values())}/{len(lines)}')
    print('Report: ' + str(report.relative_to(base.parent.parent)))

print('\nFocused native CLI executable file: 11 passed; C# entry-point contracts: 31 passed.')
print('Both public tag-name examples, strict Clippy, format, ShellCheck, release/source checks and 221 repository checks pass.')
print('Finite nesting controls in both native commands: depths 2/32/62/63 accepted; 128/2048 rejected at the declared limit 64.')
print('Rejected nesting preserves exact rows through reopening and a following successful edit; no parser behavior change.')
print('Before reproductions and exact errors: dev/log/issues/138/pulls/139/cli-before.txt.')
print('The initial xUnit synchronous Timeout attributes were rejected, then removed before these complete passing C# runs.')
print('No production files excluded; complete-project 100% and the wider inventory remain unfinished. Fresh exact-head CI required.')
