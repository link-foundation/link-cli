#!/usr/bin/env python3
"""Independent CLNKBK01 root-mode vectors; no production codec imports."""
import json
import struct
import sys
from pathlib import Path


def encode(epoch, flags, identity=bytes(range(16)), magic=b'CLNKBK01', padding=0):
    bank = bytes(a ^ b for a, b in zip(identity, struct.pack('<Q', epoch) + b'BANK0001'))
    data = bytearray(128)
    data[:8] = magic
    data[8:24] = identity
    struct.pack_into('<QII', data, 24, epoch, epoch % 2, flags)
    data[40:56] = bank
    data[56] = padding
    checksum = 0xcbf29ce484222325
    for byte in data[:120]:
        checksum = ((checksum ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    struct.pack_into('<Q', data, 120, checksum)
    return data.hex()


if __name__ == '__main__':
    epochs = [0, 1, 2, 3, 4, 2**53 + 1, 2**64 - 1]
    document = {'format': 'CLNKBK01', 'rootMode': 2,
                'decisions': [{'epoch': str(epoch), 'bankFlags': bank, 'slot': epoch % 2,
                               'bytes': encode(epoch, 2 | bank)}
                              for bank in [0, 1] for epoch in epochs],
                'foreign': encode(1, 2, bytes(reversed(range(16)))),
                'wrongMode': encode(1, 0),
                'unknownMode': encode(1, 6),
                'unknownVersion': encode(1, 2, magic=b'CLNKBK02'),
                'nonzeroPadding': encode(1, 2, padding=1)}
    destination = Path(sys.argv[1])
    destination.write_text(json.dumps(document, indent=2) + '\n')
    print('Independent mirrored checkpoint root vectors:', len(document['decisions']))
