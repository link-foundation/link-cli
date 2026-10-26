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
Path(sys.argv[1] if len(sys.argv) > 1 else "docs/protocol/engine-checkpoint-vectors.json").write_text(
    json.dumps(vectors, ensure_ascii=False, indent=2) + "\n"
)
