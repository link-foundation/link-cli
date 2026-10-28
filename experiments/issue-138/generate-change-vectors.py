"""Independent, finite engine2 fixture author; never imports production codecs.

Run from repository root to regenerate docs/protocol/engine-change-vectors.json.
The bounded reference author does not import or call production codecs.
"""
import json
from pathlib import Path


def record(text):
    value = 0xCBF29CE484222325
    for byte in text.encode("utf8"):
        value = ((value ^ byte) * 0x100000001B3) & ((1 << 64) - 1)
    return f"{text}|{value:016x}"


def change(revision, base, next_id, operations):
    return record(f"engine2|{revision}|{base}|{next_id}|{operations}")


name = "雪;|😀\0\n".encode("utf8").hex()
seed = change(1, 0, 3, f"+1,0,0,{name};+2,1,2,")
update = change(2, 1, 3, "+1,2,0,-;-2")
wide = change(3, 2, 4294967296, "+4294967295,0,4294967295,")
allocator = change(4, 3, 4294967297, "")
valid = [seed, update, wide, allocator]
mixed = [
    record(f"engine1|1|3|1,0,0,{name};2,1,2,"),
    update,
    record("engine1|3|4294967296|1,2,0,-;4294967295,0,4294967295,"),
    allocator,
]


def invalid(label, records):
    return {"case": label, "records": records}


bad = [
    invalid("missing initial revision", [update]),
    invalid("wrong base", [seed, change(2, 0, 3, "+1,2,0,-")]),
    invalid("revision gap", [seed, change(3, 1, 3, "+1,2,0,-")]),
    invalid("allocator backwards", [seed, change(2, 1, 2, "-2")]),
    invalid("repeated revision", [seed, seed]),
    invalid("duplicate operation", [seed, change(2, 1, 3, "+1,2,0,-;-1")]),
    invalid("unsorted operations", [seed, change(2, 1, 3, "-2;+1,2,0,-")]),
    invalid("absent tombstone", [seed, change(2, 1, 4, "-3")]),
    invalid("unchanged put", [seed, change(2, 1, 3, f"+1,0,0,{name}")]),
    invalid("zero address", [change(1, 0, 2, "+0,0,0,-")]),
    invalid("address beyond allocator", [change(1, 0, 2, "+2,0,0,-")]),
    invalid("zero allocator", [change(1, 0, 0, "")]),
    invalid("revision overflow", [change(18446744073709551616, 0, 2, "")]),
    invalid("allocator overflow", [change(1, 0, 340282366920938463463374607431768211456, "")]),
    invalid("address overflow", [change(1, 0, 4294967297, "+4294967296,0,0,-")]),
    invalid("source overflow", [change(1, 0, 2, "+1,4294967296,0,-")]),
    invalid("invalid UTF8", [change(1, 0, 2, "+1,0,0,ff")]),
    invalid("odd hex name", [change(1, 0, 2, "+1,0,0,f")]),
    invalid("uppercase hex name", [change(1, 0, 2, "+1,0,0,EF")]),
    invalid("nonhex name", [change(1, 0, 2, "+1,0,0,gg")]),
    invalid("unknown operation", [change(1, 0, 2, "?1")]),
    invalid("unsigned put", [change(1, 0, 2, "1,0,0,-")]),
    invalid("delete fields", [seed, change(2, 1, 3, "-2,0")]),
    invalid("trailing operation", [change(1, 0, 2, "+1,0,0,-;")]),
    invalid("empty operation", [change(1, 0, 2, ";+1,0,0,-")]),
    invalid("missing put field", [change(1, 0, 2, "+1,0,0")]),
    invalid("extra put field", [change(1, 0, 2, "+1,0,0,-,0")]),
    invalid("extra header field", [record("engine2|1|0|2|+1,0,0,-|extra")]),
    invalid("short header", [record("engine2|1|2|+1,0,0,-")]),
    invalid("unknown mandatory format", [record("engine3|1|0|2|+1,0,0,-")]),
]
for field, text in [
    ("revision leading zero", "engine2|01|0|2|+1,0,0,-"),
    ("base leading zero", "engine2|1|00|2|+1,0,0,-"),
    ("allocator leading zero", "engine2|1|0|02|+1,0,0,-"),
    ("address leading zero", "engine2|1|0|2|+01,0,0,-"),
    ("signed address", "engine2|1|0|2|++1,0,0,-"),
    ("source whitespace", "engine2|1|0|2|+1, 0,0,-"),
    ("non ASCII decimal", "engine2|1|0|2|+1,０,0,-"),
    ("embedded NUL", "engine2|1|0|2|+1,0\0,0,-"),
]:
    bad.append(invalid(field, [record(text)]))
bad.extend([
    invalid("wrong checksum", [seed[:-1] + ("0" if seed[-1] != "0" else "1")]),
    invalid("uppercase checksum", [seed[:-16] + seed[-16:].upper()]),
    invalid("short checksum", [seed[:-1]]),
])
# Independent physical compression controls: only adjacent complete images may
# use delta1. The dependent logical record breaks that compression dependency.
def full(revision, rows):
    payload = ";".join(f"{id},{source},0,{name}" for id, (source, name) in sorted(rows.items()))
    return record(f"engine1|{revision}|33|{payload}")


def compress(before, after):
    a = before.split("|"); b = after.split("|")
    prefix = 0
    while prefix < min(len(a[3]), len(b[3])) and a[3][prefix] == b[3][prefix]:
        prefix += 1
    suffix = 0
    while suffix < min(len(a[3]), len(b[3])) - prefix and a[3][-1-suffix] == b[3][-1-suffix]:
        suffix += 1
    middle = b[3][prefix:len(b[3])-suffix]
    candidate = record(f"delta1|{a[4]}|{b[1]}|{b[2]}|{prefix}|{suffix}|{middle}|{b[4]}")
    assert len(candidate) < len(after)
    return candidate


rows = {id: (0, (f"named row {id} " + "雪" * 8).encode("utf8").hex()) for id in range(1, 33)}
one = full(1, rows)
rows[1] = (1, rows[1][1]); two = full(2, rows)
rows[1] = (2, rows[1][1]); del rows[2]
three = change(3, 2, 33, f"+1,2,0,{rows[1][1]};-2")
rows[1] = (3, rows[1][1]); four = full(4, rows)
rows[1] = (4, rows[1][1]); five = full(5, rows)
physical_mixed = {"logical": [one, two, three, four, five],
                  "physical": [one, compress(one, two), three, four, compress(four, five)]}
invalid_physical = [
    invalid("physical delta cannot use a logical change as its base", [seed, compress(one, two)]),
    invalid("logical base must precede revision", [seed, change(2, 0, 3, "+1,2,0,-")]),
    invalid("short logical header", [record("engine2|1|2|+1,0,0,-")]),
]
output = {"valid": valid, "mixed": mixed, "invalidU32": bad,
          "physicalMixed": physical_mixed, "invalidPhysical": invalid_physical}
(Path(__file__).resolve().parents[2] / "docs/protocol/engine-change-vectors.json").write_text(
    json.dumps(output, indent=2, ensure_ascii=False) + "\n")
print(f"Authored {len(valid)} change records, {len(mixed)} mixed records and {len(bad)} invalid streams")
