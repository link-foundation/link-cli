#!/usr/bin/env python3
"""Independent blob2 epoch/content-address wire vectors; no production imports."""
import json
import sys
from pathlib import Path

LIMIT = (1 << 64) - 1
def object_key(epoch, records):
    data = b'blob2|' + b''.join(str(len(item.encode())).encode() + b':' + item.encode() for item in records)
    value = 0xcbf29ce484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001b3) & LIMIT
    return f'blob2-{epoch:016x}-{value:016x}'

records = [[], [''], ['', ''], ['ab', 'c'], ['a', 'bc'], ['雪😀'],
           ['\ufeff\0\n', 'é'], ['0:', '雪', ''], ['one', 'two'], ['\U00010000', '\uffff']]
objects = [dict(epoch=str(epoch), entries=items, key=object_key(epoch, items))
           for epoch in (0, 1, LIMIT) for items in records]
epochs = [dict(entries=['gc1', str(value)], valid=True) for value in (0, 1, LIMIT)]
epochs += [dict(entries=value, valid=False) for value in (
    [], ['gc1'], ['gc1', '0', 'extra'], ['gc0', '0'], ['gc1', ''], ['gc1', '00'],
    ['gc1', '+1'], ['gc1', '-1'], ['gc1', ' 1'], ['gc1', str(LIMIT + 1)])]
base = ['blob2', 'east', object_key(0, ['live 雪']), '7']
pointers = [dict(entries=base, objectEntries=['live 雪'], valid=True),
            dict(entries=['blob2', 'west', object_key(LIMIT, []), str(LIMIT)], objectEntries=[], valid=True)]
for index, values in ((0, ['blob1', 'blob3', '']), (1, ['', '../east', 'missing']),
                      (2, ['', 'blob2-0-0000000000000000', base[2].upper(), base[2] + '0', base[2][:22] + '_' + base[2][23:]]),
                      (3, ['0', '00', '01', '+7', '-1', str(LIMIT + 1)])):
    for value in values:
        item = base.copy(); item[index] = value
        pointers.append(dict(entries=item, valid=False))
for item in ([], base[:-1], base + ['extra']):
    pointers.append(dict(entries=item, valid=False))
result = dict(objects=objects, epochs=epochs, pointers=pointers)
output = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('docs/protocol/reclamation-v1.json')
output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
print(f'Independent vectors: {len(objects)} addresses, {len(epochs)} epoch cases, {len(pointers)} pointer cases; compare with actual public APIs.')
