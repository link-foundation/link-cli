#!/usr/bin/env python3
"""Independent, finite delta1 oracle; no implementation imports."""
import json
from pathlib import Path


def checksum(text):
    value = 0xCBF29CE484222325
    for byte in text.encode("ascii"):
        value = ((value ^ byte) * 0x100000001B3) & ((1 << 64) - 1)
    return f"{value:016x}"


def checked(text):
    return text + "|" + checksum(text)


def full(revision, rows):
    payload = ";".join(f"{i},0,0,{name}" for i, name in sorted(rows.items()))
    return checked(f"engine1|{revision}|201|{payload}")


def delta(before, after):
    old, new = before.split("|"), after.split("|")
    a, b = old[3], new[3]
    prefix = 0
    while prefix < min(len(a), len(b)) and a[prefix] == b[prefix]:
        prefix += 1
    suffix = 0
    while suffix < min(len(a), len(b)) - prefix and a[-suffix-1] == b[-suffix-1]:
        suffix += 1
    middle = b[prefix:len(b)-suffix] if suffix else b[prefix:]
    return checked("|".join(["delta1", old[4], new[1], new[2], str(prefix),
                             str(suffix), middle, new[4]]))


rows = {i: "-" for i in range(1, 201)}
first = full(1, rows)
rows[100] = "雪;|😀".encode().hex()
second = full(2, rows)
del rows[3]
third = full(3, rows)
d2, d3 = delta(first, second), delta(second, third)
invalid = []


def bad(name, records):
    invalid.append({"name": name, "records": records})


def mutate(name, field, value):
    fields = d2.split("|")[:-1]
    fields[field] = value
    bad(name, [first, checked("|".join(fields))])


bad("missing base", [d2])
bad("wrong base", [full(1, {1: "-"}), d2])
bad("bad delta checksum", [first, d2[:-1] + ("0" if d2[-1] != "0" else "1")])
bad("bad full checksum", [first[:-1] + ("0" if first[-1] != "0" else "1")])
bad("missing delta field", [first, checked("delta1|1")])
bad("missing full field", [checked("engine1|1|201")])
bad("first revision not one", [second])
bad("repeated full revision", [first, first])
bad("noncontiguous full revision", [first, third])
bad("unknown record type", [checked("other|1|201|")])
bad("non-ASCII full", [checked("engine1|1|201|").replace("201", "雪")])
bad("missing checksum separator", ["engine1"])
for name, field, value in [
    ("zero full revision", 1, "0"),
    ("zero full allocator", 2, "0"),
    ("empty full revision", 1, ""),
    ("nondecimal full revision", 1, "a"),
    ("u128 parser overflow", 2, str(1 << 128)),
]:
    fields = first.split("|")[:-1]
    fields[field] = value
    bad(name, [checked("|".join(fields))])
for name, field, value in [
    ("wrong base hash", 1, "0" * 16),
    ("bad result hash", 7, "0" * 16),
    ("short result hash", 7, "0"),
    ("nonhex result hash", 7, "g" * 16),
    ("revision gap", 2, "3"),
    ("zero revision", 2, "0"),
    ("signed revision", 2, "+2"),
    ("noncanonical revision", 2, "02"),
    ("revision overflow", 2, str(1 << 64)),
    ("zero allocator", 3, "0"),
    ("allocator overflow", 3, str(1 << 128)),
    ("noncanonical allocator", 3, "0201"),
    ("negative prefix", 4, "-1"),
    ("oversized decimal", 4, "1" * 40),
    ("prefix overflow", 4, str(1 << 32)),
    ("prefix outside payload", 4, "4294967295"),
    ("noncanonical prefix", 4, "00"),
    ("negative suffix", 5, "-1"),
    ("suffix overlap", 5, "4294967295"),
    ("suffix overflow", 5, str(1 << 32)),
    ("incorrect middle", 6, "bad"),
]:
    mutate(name, field, value)

output = {"format": "delta1", "validSequences": [
    {"physical": [first, d2, d3], "logical": [first, second, third]},
    {"physical": [first, second, d3], "logical": [first, second, third]},
    {"physical": [first, second, third], "logical": [first, second, third]},
    {"physical": [], "logical": []},
    {"physical": [checked("engine1|1|1|"), checked("engine1|2|1|")],
     "logical": [checked("engine1|1|1|"), checked("engine1|2|1|")]},
], "invalidSequences": invalid}
root = Path(__file__).resolve().parents[2]
(root / "docs/protocol/engine-delta-vectors.json").write_text(
    json.dumps(output, ensure_ascii=False, indent=2) + "\n")
print(f"Wrote {len(output['validSequences'])} valid and {len(invalid)} invalid sequences")
