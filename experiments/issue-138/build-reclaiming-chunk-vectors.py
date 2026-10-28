#!/usr/bin/env python3
"""Independent chunks2 UTF-8 framing oracle; no production import."""
import json
from pathlib import Path
import sys


def key(epoch, records):
    h = 14695981039346656037
    data = b'blob2|' + b''.join(str(len(s.encode('utf-8'))).encode('ascii') + b':' + s.encode('utf-8') for s in records)
    for b in data:
        h = ((h ^ b) * 1099511628211) % (2 ** 64)
    return f'blob2-{epoch:016x}-{h:016x}'


def vector(epoch, records, count, maximum):
    chunks, current, used = [], [], 0
    for record in records:
        n = len(record.encode('utf-8'))
        assert n <= maximum
        if current and (len(current) == count or n > maximum - used):
            chunks.append(current)
            current, used = [], 0
        current.append(record)
        used += n
    if current:
        chunks.append(current)
    return dict(epoch=str(epoch), entries=records, maxRecords=count, maxBytes=maximum,
                chunks=[dict(entries=chunk, address=key(epoch, chunk)) for chunk in chunks])


cases = [([], 2, 8), ([''], 2, 8), (['a', 'b', 'c', 'd', 'e'], 2, 8),
         (['é', '😀', '', '\ufeff\0', 'tail'], 3, 6),
         (['ab', 'c', 'é', '', 'x'], 9, 3), (['😀'], 1, 4)]
vectors = [vector(epoch, *case) for epoch in [0, 1, 2**64-1] for case in cases]
path = Path(sys.argv[1] if len(sys.argv) > 1 else 'docs/protocol/reclaiming-chunks-vectors.json')
path.write_text(json.dumps(vectors, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print(f'{len(vectors)} independent chunks2 vectors written to {path}')
