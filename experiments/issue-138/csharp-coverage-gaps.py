"""Print bounded source gaps from a Coverlet Cobertura report without exclusions."""
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
report = ET.parse(sys.argv[1]).getroot()
print({key: report.attrib[key] for key in ['lines-covered', 'lines-valid', 'branches-covered', 'branches-valid']})
files = {}
for cls in report.findall('.//class'):
    filename = cls.attrib['filename']
    if sys.argv[2] not in filename:
        continue
    for line in cls.findall('./lines/line'):
        state = files.setdefault(filename, {}).setdefault(int(line.attrib['number']), [0, []])
        state[0] += int(line.attrib['hits'])
        if line.attrib.get('branch', '').lower() == 'true':
            state[1].append((cls.attrib['name'], line.attrib.get('condition-coverage', '')))
for filename, lines in sorted(files.items()):
    path = Path(filename)
    if not path.exists():
        path = Path('csharp') / path
    source = path.read_text().splitlines() if path.exists() else []
    print('\n' + filename)
    for number, (hits, branches) in sorted(lines.items()):
        partial = [(name, condition) for name, condition in branches if not condition.startswith('100%')]
        if not hits or partial:
            text = source[number-1].strip() if number <= len(source) else ''
            print(f'{number}: hits={hits}, partial={partial}: {text}')
