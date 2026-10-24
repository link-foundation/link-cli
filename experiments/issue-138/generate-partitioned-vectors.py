"""Independent portable object-address oracle, including framing and UTF-8."""
import json
from pathlib import Path

vectors = []
for entries in [[], [""], ["ab", "c"], ["a", "bc"], ["é", "雪😀"], ["a\0b", "\n|:"]]:
    data = bytearray(b"blob1|")
    for entry in entries:
        encoded = entry.encode("utf-8")
        data.extend(str(len(encoded)).encode("ascii") + b":" + encoded)
    value = 0xCBF29CE484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    vectors.append({"entries": entries, "address": f"blob1-{value:016x}"})
Path("docs/protocol/partitioned-journal-vectors.json").write_text(
    json.dumps(vectors, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
)
