"""Independent reference encoder for the engine1 compatibility fixtures."""
import json
from pathlib import Path


def record(text):
    value = 0xCBF29CE484222325
    for byte in text.encode("utf8"):
        value = ((value ^ byte) * 0x100000001B3) & ((1 << 64) - 1)
    return f"{text}|{value:016x}"


valid = [
    record("engine1|1|2|1,0,0," + "雪;|😀".encode("utf8").hex()),
    record("engine1|2|2|"),
    record("engine1|3|4294967296|4294967295,0,4294967295,"),
]
invalid = [
    "bad", valid[0] + "0", record("engine2|1|2|1,0,0,-"),
    record("engine1|bad|2|"), record("engine1|1|bad|"), record("engine1|1|0|"),
    record("engine1|1|2|1,0"), record("engine1|1|2|0,0,0,-"),
    record("engine1|1|2|2,0,0,-"), record("engine1|1|2|1,0,0,-;1,0,0,-"),
    record("engine1|1|2|1,0,0,f"), record("engine1|1|2|1,0,0,zz"),
    record("engine1|1|2|1,0,0,ff"), record("engine1|1|2|1,bad,0,-"),
    record("engine1|1|2|1,0,4294967296,-"),
    record("engine1|+1|2|1,0,0,-"), record("engine1|1|+2|1,0,0,-"),
    record("engine1|1\0|2|1,0,0,-"), record("engine1|1|2|1,0\0,0,-"),
    record("engine1|1|2|1,0,0,é"),
    record("engine1|1|2|1,0,0,+a"),
    record("engine1|18446744073709551616|2|"),
    record("engine1|1|340282366920938463463374607431768211456|"),
]
for value in range(256):
    candidate = record(f"engine1|1|2|1,0,{value},-")
    if candidate[-16] == '0':
        invalid += [candidate[:-16] + prefix + candidate[-15:] for prefix in [' ', '+']]
        break
invalid.append(valid[0][:-1] + ('0' if valid[0][-1] != '0' else '1'))
Path("docs/protocol/engine-revision-vectors.json").write_text(json.dumps({"valid": valid, "invalidU32": invalid}, indent=2) + "\n")
