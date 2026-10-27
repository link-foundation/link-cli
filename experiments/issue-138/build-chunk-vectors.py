#!/usr/bin/env python3
"""Independent finite UTF-8/framing oracle; never imports production codecs."""
import json
import sys
from pathlib import Path


def address(records):
    payload = b'blob1|' + b''.join(str(len(r.encode('utf-8'))).encode('ascii') + b':' + r.encode('utf-8') for r in records)
    value = 0xcbf29ce484222325
    for byte in payload:
        value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return f'blob1-{value:016x}'


def vector(records, count, size):
    chunks, current, used = [], [], 0
    for record in records:
        length = len(record.encode('utf-8'))
        assert length <= size
        if current and (len(current) == count or length > size - used):
            chunks.append(current)
            current, used = [], 0
        current.append(record)
        used += length
    if current:
        chunks.append(current)
    return dict(entries=records, maxRecords=count, maxBytes=size,
                chunks=[dict(entries=chunk, address=address(chunk)) for chunk in chunks])


vectors = [vector([], 2, 8), vector([''], 2, 8), vector(['a', 'b', 'c', 'd', 'e'], 2, 8),
           vector(['é', '😀', '', '\ufeff\0', 'tail'], 3, 6),
           vector(['ab', 'c', 'é', '', 'x'], 9, 3), vector(['😀'], 1, 4)]
path = Path(sys.argv[1] if len(sys.argv) > 1 else 'docs/protocol/chunked-journal-vectors.json')
path.write_text(json.dumps(vectors, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print(f'{len(vectors)} independent vectors written to {path}')
