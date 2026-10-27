#!/usr/bin/env python3
"""Independent finite input/output fixtures; does not import a runtime codec."""
import copy
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
A, B, C = (f"{n:032x}" for n in (1, 2, 3))
MAX = "18446744073709551615"


def version(address="1", author=A, counter="1", clock=None, payload=None):
    return ["causal-version1", address, [author, counter],
            clock if clock is not None else [[author, counter]], payload]


def text(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=True)


base = version(payload=["2", "3", None])
fresh = copy.deepcopy(base)
update = version(counter="2", payload=["4", "5", "6e6577"])
deleted = version(author=B, clock=[[A, "1"], [B, "1"]])
resolved = version(counter="3", clock=[[A, "3"], [B, "1"]],
                   payload=["4", "5", "7265736f6c766564"])
cases = []


def valid(name, record):
    cases.append({"name": name, "valid": True, "record": text(record),
                  "canonical": text(record)})


def invalid(name, record):
    cases.append({"name": name, "valid": False,
                  "record": record if isinstance(record, str) else text(record)})


valid("ordinary", base)
valid("tombstone", deleted)
valid("maximum-address-counter-and-endpoints",
      version(MAX, counter=MAX, payload=[MAX, "0", ""]))
valid("unicode-null-newline-and-bom-name",
      version(payload=["0", "0", "雪😀\x00\n\ufeff".encode().hex()]))
valid("distinct-empty-name", version(payload=["0", "0", ""]))
valid("maximum-name-bytes", version(payload=["0", "0", "61" * 4096]))
valid("maximum-clock-authors", version(clock=[[f"{i:032x}", "1"] for i in range(1, 65)]))

for name, index, bad in [
    ("wrong-format", 0, "causal-version2"),
    ("zero-address", 1, "0"), ("numeric-address", 1, 1),
    ("leading-zero-address", 1, "01"), ("negative-address", 1, "-1"),
    ("overflow-address", 1, "18446744073709551616"),
    ("empty-address", 1, ""), ("dot-shape", 2, [A]),
    ("zero-author", 2, ["0" * 32, "1"]),
    ("uppercase-author", 2, ["a" * 31 + "B", "1"]),
    ("short-author", 2, ["1", "1"]),
    ("zero-counter", 2, [A, "0"]),
    ("numeric-counter", 2, [A, 1]),
    ("overflow-counter", 2, [A, "18446744073709551616"]),
    ("empty-clock", 3, []), ("nonarray-clock", 3, {}),
    ("duplicate-author", 3, [[A, "1"], [A, "1"]]),
    ("unsorted-clock", 3, [[B, "1"], [A, "1"]]),
    ("clock-zero", 3, [[A, "0"]]),
    ("clock-shape", 3, [[A, "1", "extra"]]),
    ("dot-absent", 3, [[B, "1"]]),
    ("dot-counter-mismatch", 3, [[A, "2"]]),
    ("too-many-authors", 3, [[f"{i:032x}", "1"] for i in range(1, 66)]),
    ("wrong-payload-shape", 4, ["0", "0"]),
    ("numeric-endpoint", 4, [0, "0", None]),
    ("overflow-endpoint", 4, [MAX + "0", "0", None]),
    ("name-nonstring", 4, ["0", "0", 1]),
    ("uppercase-name-hex", 4, ["0", "0", "AB"]),
    ("odd-name-hex", 4, ["0", "0", "a"]),
    ("invalid-utf8-name", 4, ["0", "0", "ff"]),
    ("utf8-surrogate-name", 4, ["0", "0", "eda080"]),
    ("name-too-large", 4, ["0", "0", "61" * 4097]),
]:
    record = copy.deepcopy(base)
    record[index] = bad
    invalid(name, record)
invalid("extra-top-level-field", base + [None])
invalid("top-level-object", {})
invalid("trailing-record", text(base) + text(base))
invalid("physical-newline", text(base) + "\n")
invalid("noncanonical-whitespace", json.dumps(base))
invalid("noncanonical-unicode-escape", text(base).replace("causal", "\\u0063ausal"))
invalid("wire-too-large", " " * 65537)

merges = [
    {"name": "empty", "left": [], "right": [], "expected": []},
    {"name": "idempotent-delivery", "left": [base], "right": [fresh], "expected": [base]},
    {"name": "observed-update", "left": [base], "right": [update], "expected": [update]},
    {"name": "retained-deletion", "left": [base], "right": [deleted], "expected": [deleted]},
    {"name": "concurrent-update-delete", "left": [update], "right": [deleted], "expected": [update, deleted]},
    {"name": "observed-resolution", "left": [update, deleted], "right": [resolved], "expected": [resolved]},
]
for name, other in [
    ("same-dot-different-payload", version(payload=["2", "4", None])),
    ("same-dot-different-context", version(clock=[[A, "1"], [B, "1"]], payload=base[4])),
    ("same-clock-different-dot", version(author=B, clock=[[A, "1"], [B, "1"]])),
    ("different-address", version(address="2", payload=base[4])),
]:
    left = deleted if name == "same-clock-different-dot" else base
    if name == "same-clock-different-dot":
        other = version(clock=[[A, "1"], [B, "1"]])
    merges.append({"name": name, "left": [left], "right": [other], "valid": False})
# Check contradictory duplicate dots before dominance removes both of them.
merges.append({"name": "dominated-contradiction", "left": [base, resolved],
               "right": [version(payload=["7", "8", None])], "valid": False})
advances = [
    {"name": "first-tombstone", "address": "1", "author": A, "lastCounter": "0",
     "observed": [], "payload": None, "expected": version()},
    {"name": "observed-sibling-resolution", "address": "1", "author": A, "lastCounter": "2",
     "observed": [update, deleted], "payload": resolved[4], "expected": resolved},
    {"name": "retained-global-counter", "address": "1", "author": A, "lastCounter": "7",
     "observed": [base], "payload": None, "expected": version(counter="8")},
    {"name": "observed-counter-floor", "address": "1", "author": A, "lastCounter": "0",
     "observed": [update], "payload": None, "expected": version(counter="3")},
    {"name": "final-counter", "address": "1", "author": A, "lastCounter": str(int(MAX)-1),
     "observed": [], "payload": None, "expected": version(counter=MAX)},
    {"name": "exhausted-local-counter", "address": "1", "author": A, "lastCounter": MAX,
     "observed": [], "payload": None, "valid": False},
    {"name": "exhausted-observed-counter", "address": "1", "author": A, "lastCounter": "0",
     "observed": [version(counter=MAX)], "payload": None, "valid": False},
    {"name": "wrong-address", "address": "2", "author": A, "lastCounter": "0",
     "observed": [base], "payload": None, "valid": False},
    {"name": "joined-clock-author-limit", "address": "1", "author": f"{65:032x}", "lastCounter": "0",
     "observed": [version(clock=[[f"{i:032x}", "1"] for i in range(1, 65)])],
     "payload": None, "valid": False},
]
result = {"format": "causal-version-v1-vectors", "authors": [A, B, C],
          "cases": cases, "merges": merges,
          "events": [base, update, deleted, resolved], "advances": advances}
target = ROOT / "docs/protocol/causal-version-v1.json"
target.write_text(json.dumps(result, indent=2, ensure_ascii=True) + "\n")
print(f"Wrote {len(cases)} codec, {len(merges)} merge and {len(advances)} advance cases: {target.relative_to(ROOT)}")
