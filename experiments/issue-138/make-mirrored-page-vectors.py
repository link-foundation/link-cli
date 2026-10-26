"""Independent mirrored-mode decisions and finite corruption instructions.

Starts from independently generated v1 bytes; imports no production codec.
The page layout is unchanged. Only the immutable metadata mode and its checksum
differ. Tests apply instructions to one selected copy of the healthy image.
"""
import argparse
import json
import struct
from pathlib import Path


def fnv(data):
    value = 14695981039346656037
    for byte in data:
        value = ((value ^ byte) * 1099511628211) & ((1 << 64) - 1)
    return value


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', nargs='?', default='experiments/issue-138/mirrored-page-vectors.json')
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
result = json.loads((root / 'docs/protocol/native-pages-v1.json').read_text())
for name in ['empty', 'oneGroup', 'twoGroups']:
    image = result[name]
    for slot, text in enumerate(image['metadata']):
        if not text:
            continue
        data = bytearray.fromhex(text)
        assert fnv(data[:120]) == struct.unpack_from('<Q', data, 120)[0]
        assert struct.unpack_from('<II', data, 8) == (1, 0)
        struct.pack_into('<I', data, 12, 1)
        struct.pack_into('<Q', data, 120, fnv(data[:120]))
        image['metadata'][slot] = data.hex()

result['singleCopyFaults'] = [
    dict(kind='page', side=side, page=page, offset=offset)
    for side in [0, 1] for page in [0, 1] for offset in [0, 56, 64, 4095]
] + [
    dict(kind='metadata', side=side, slot=0, length=length)
    for side in [0, 1] for length in [0, 1, 8, 64, 119, 120, 127]
] + [
    dict(kind='truncated', side=side, pages=pages)
    for side in [0, 1] for pages in [0, 1]
] + [dict(kind='stale', side=side, page=1) for side in [0, 1]]
Path(args.output).write_text(json.dumps(result, ensure_ascii=True, indent=2) + '\n')
print('Independent mirrored metadata and 36 finite single-copy fault vectors')
