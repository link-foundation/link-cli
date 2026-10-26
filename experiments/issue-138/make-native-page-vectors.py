"""Independent finite wire vectors; no production codec is imported."""
import json
import struct
import sys
from pathlib import Path

PAGE = 4096
CELLS = 252
IDENTITY = bytes(range(16))
MASK = (1 << 64) - 1


def hash_bytes(data):
    result = 0xcbf29ce484222325
    for value in data:
        result = ((result ^ value) * 0x100000001b3) & MASK
    return result


def metadata(generation, pages, rows, records, root, last_hash):
    result = bytearray(128)
    result[:8] = b'LNKDMT01'
    struct.pack_into('<II', result, 8, 1, 0)
    result[16:32] = IDENTITY
    struct.pack_into('<6Q', result, 32, generation, pages, rows, records, root, last_hash)
    struct.pack_into('<Q', result, 120, hash_bytes(result[:120]))
    return result


def append_group(image, group, max_cells=CELLS):
    generation = image['generation'] + 1
    slots = []
    pages = []
    root = image['root']
    rows = image['rows']
    previous_hash = image['last_hash']

    def finish():
        nonlocal previous_hash
        page = bytearray(PAGE)
        page[:8] = b'LNKDPG01'
        page[8:24] = IDENTITY
        struct.pack_into('<QQIIQ', page, 24, len(image['pages']) + len(pages), generation, len(slots), 0, previous_hash)
        for position, pair in enumerate(slots):
            struct.pack_into('<QQ', page, 64 + 16 * position, *pair)
        previous_hash = hash_bytes(page[:56] + page[64:])
        struct.pack_into('<Q', page, 56, previous_hash)
        pages.append(page)
        slots.clear()

    def cell(source, target):
        nonlocal rows
        if len(slots) == max_cells:
            finish()
        index = (len(image['pages']) + len(pages)) * CELLS + len(slots) + 1
        slots.append((source, target))
        rows += 1
        return index

    for record in group:
        assert '\r' not in record and '\n' not in record
        text = record.encode('utf-8')
        chunks = [text[start:start + 7] for start in range(0, len(text), 7)]
        string = 0
        for chunk in reversed(chunks):
            external = (1 << 63) | (len(chunk) << 56) | int.from_bytes(chunk, 'little')
            string = cell(external, string)
        root = cell(string, root)
    finish()
    image.update(generation=generation, rows=rows, root=root, last_hash=previous_hash)
    image['pages'].extend(pages)
    image['records'].extend(group)
    image['metadata'][generation % 2] = metadata(generation, len(image['pages']), rows, len(image['records']), root, previous_hash)


def snapshot(image):
    return {'records': image['records'][:], 'metadata': [value.hex() for value in image['metadata']],
            'pages': [value.hex() for value in image['pages']]}



if __name__ == "__main__":
    image = dict(generation=0, root=0, rows=0, last_hash=0, pages=[], records=[],
                 metadata=[metadata(0, 0, 0, 0, 0, 0), bytes()])
    result = {'databaseId': IDENTITY.hex(), 'empty': snapshot(image)}
    append_group(image, ['', 'snow 雪', '\ufeff\0'])
    result['oneGroup'] = snapshot(image)
    append_group(image, [''])
    result['twoGroups'] = snapshot(image)
    Path(sys.argv[1] if len(sys.argv) > 1 else 'experiments/issue-138/native-page-vectors.json').write_text(json.dumps(result, ensure_ascii=True, indent=2) + '\n')
    print('Independent vectors: empty, UTF-8 multi-chunk group, immutable partial-page successor')
