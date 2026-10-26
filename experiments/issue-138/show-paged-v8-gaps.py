"""Print uncovered V8 branch spans; do not change any coverage denominator."""
import json
from pathlib import Path
from urllib.parse import urlparse, unquote
import sys
reports = {}
for file in Path(sys.argv[1]).glob('*.json'):
    for source in json.loads(file.read_text())['result']:
        if source['url'].endswith(('src/engine-paged-log.js', 'src/engine-file-paged-log.js')):
            for function in source['functions']:
                for span in function['ranges']:
                    key = (source['url'], span['startOffset'], span['endOffset'])
                    reports[key] = reports.get(key, 0) + span['count']
for (url, start, end), count in sorted(reports.items()):
    if count == 0:
        text = Path(unquote(urlparse(url).path)).read_text()
        print(f"{Path(url).name}:{text[:start].count(chr(10)) + 1}-{text[:end].count(chr(10)) + 1}: {text[start:end][:300]!r}")
