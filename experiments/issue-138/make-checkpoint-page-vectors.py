"""Independent bounded two-bank root vectors; imports no production codec."""
import json
import struct
import sys
from pathlib import Path


def digest(data):
    value = 0xcbf29ce484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return value


identity = bytes(range(16))


def bank_id(epoch, root_identity=identity):
    mask = struct.pack('<Q', epoch) + b'BANK0001'
    return bytes(a ^ b for a, b in zip(root_identity, mask))


def decision(epoch, flags=0, root_identity=identity):
    result = bytearray(128)
    result[:8] = b'CLNKBK01'
    result[8:24] = root_identity
    struct.pack_into('<QII', result, 24, epoch, epoch % 2, flags)
    result[40:56] = bank_id(epoch, root_identity)
    struct.pack_into('<Q', result, 120, digest(result[:120]))
    return result


def empty_bank(epoch, flags=0):
    value = bytearray(128)
    value[:8] = b'LNKDMT01'
    struct.pack_into('<II', value, 8, 1, flags)
    value[16:32] = bank_id(epoch)
    struct.pack_into('<Q', value, 120, digest(value[:120]))
    return value.hex()


epochs = [0, 1, 2, 3, (1 << 53) + 1, (1 << 64) - 1]
result = {'identity': identity.hex(), 'decisions': {}, 'emptyBanks': {}}
for epoch in epochs:
    result['decisions'][str(epoch)] = decision(epoch).hex()
    result['emptyBanks'][str(epoch)] = empty_bank(epoch)
result['mirroredDecision'] = decision(1, 1).hex()
result['mirroredEmptyBanks'] = [empty_bank(epoch, 1) for epoch in [0, 1]]
result['foreignDecision'] = decision(1, root_identity=bytes(reversed(identity))).hex()
invalid = []
for offset, replacement in [(0, b'OTHER001'), (32, struct.pack('<I', 1)),
                            (36, struct.pack('<I', 2)), (40, bytes(16)), (56, b'\x01')]:
    value = decision(0)
    value[offset:offset + len(replacement)] = replacement
    struct.pack_into('<Q', value, 120, digest(value[:120]))
    invalid.append(value.hex())
result['invalid'] = invalid
Path(sys.argv[1] if len(sys.argv) > 1 else 'docs/protocol/checkpoint-pages-v1.json').write_text(
    json.dumps(result, indent=2) + '\n')
print('Independent checkpoint vectors: six u64 epochs, empty banks, foreign identity, mirrored flags and five malformed checksummed decisions.')
