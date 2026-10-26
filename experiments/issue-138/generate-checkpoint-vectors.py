#!/usr/bin/env python3
"""Independent, finite engine3 wire vectors; imports no production codec."""
import json
import sys
from pathlib import Path


def record(text):
    value = 0xCBF29CE484222325
    for byte in text.encode("utf-8"):
        value = ((value ^ byte) * 0x100000001B3) & ((1 << 64) - 1)
    return f"{text}|{value:016x}"


base = "engine3|9|8|1,0,7,e99baa;7,1,0,"
vectors = {
    "valid": [
        [record("engine3|0|1|")],
        [record("engine3|18446744073709551615|1|")],
        [record(base)],
        [record(base), record("engine1|10|8|1,7,0,e99baa;7,1,0,")],
        [record(base), record("engine2|10|9|8|+1,7,0,e99baa")],
    ],
    "invalidU32": [
        [record("engine3|0|2|")],
        [record("engine3|0|2|1,0,0,-")],
        [record("engine3|-1|1|")],
        [record("engine3|18446744073709551616|1|")],
        [record("engine3|9|0|")],
        [record("engine3|9|8|0,0,0,-")],
        [record("engine3|9|8|8,0,0,-")],
        [record("engine3|9|8|1,0,0,ff")],
        [record("engine3|9|8|1,0,0,-;1,0,0,-")],
        [record("engine3|9|8|1,4294967296,0,-")],
        [record(base)[:-1] + "0"],
        [record(base), record("engine3|10|8|1,0,0,-")],
        [record(base), record("engine1|11|8|")],
        [record(base), record("engine1|10|7|1,0,0,-")],
        [record("engine1|1|2|1,0,0,-"), record("engine3|2|2|1,0,0,-")],
    ],
    "constraintViolations": {
        "references": [record("engine3|9|8|1,7,0,-")],
        "uniquePairs": [record("engine3|9|8|1,0,0,-;7,0,0,-")],
        "uniqueNames": [record("engine3|9|8|1,0,0,78;7,0,0,78")],
    },
}


def complete(kind, revision, rows):
    payload = ";".join(f"{i},{source},0,{name}" for i, (source, name) in sorted(rows.items()))
    return record(f"{kind}|{revision}|41|{payload}")


def delta(before, after):
    a, b = before.split("|"), after.split("|")
    prefix = 0
    while prefix < min(len(a[3]), len(b[3])) and a[3][prefix] == b[3][prefix]:
        prefix += 1
    suffix = 0
    while suffix < min(len(a[3]), len(b[3])) - prefix and a[3][-suffix-1] == b[3][-suffix-1]:
        suffix += 1
    middle = b[3][prefix:len(b[3])-suffix] if suffix else b[3][prefix:]
    result = record(f"delta1|{a[4]}|{b[1]}|{b[2]}|{prefix}|{suffix}|{middle}|{b[4]}")
    assert len(result) < len(after)
    return result


# Logical checkpoints and physical delta checkpoints are independent selections.
# A logical change interrupts the complete-image compression dependency.
rows = {i: (0, (f"row {i} 雪;|😀" * 2).encode().hex()) for i in range(1, 33)}
seed = complete("engine3", 9, rows)
rows[16] = (3, rows[16][1]); two = complete("engine1", 10, rows)
rows[16] = (4, rows[16][1]); three = record(f"engine2|11|10|41|+16,4,0,{rows[16][1]}")
rows[16] = (5, rows[16][1]); four = complete("engine1", 12, rows)
rows[16] = (6, rows[16][1]); five = complete("engine1", 13, rows)
zero, maximum = vectors["valid"][0][0], vectors["valid"][1][0]
vectors["physicalDelta"] = {
    "logical": [seed, two, three, four, five],
    "physical": [seed, delta(seed, two), three, four, delta(four, five)],
    "invalid": [
        [record("engine3|0|1|1,0,0,-")],
        [record("engine3|0|2|")],
        [record("engine3|9|0|")],
        [zero, record("engine3|1|1|")],
        [seed, complete("engine3", 10, rows)],
        [record("engine1|1|1|"), record("engine3|2|1|")],
        [maximum, record("engine1|1|1|")],
        [record("engine1|9|41|")],
        [seed, record("engine1|11|41|")],
        [seed, record("engine2|10|8|41|")],
        [seed, two, three, delta(two, four)],
    ],
    "zeroFollow": [zero, record("engine1|1|1|")],
}
Path(sys.argv[1] if len(sys.argv) > 1 else "docs/protocol/engine-checkpoint-vectors.json").write_text(
    json.dumps(vectors, ensure_ascii=False, indent=2) + "\n"
)
