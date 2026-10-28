#!/usr/bin/env python3
"""Independent finite receipt-admission vectors; imports no production codec."""
import copy
import argparse
import json
from pathlib import Path

root = Path(__file__).resolve().parents[2]
A, B = (f"{number:032x}" for number in (1, 2))
wire = lambda value: json.dumps(value, separators=(",", ":"), ensure_ascii=True)
token = lambda number: f"{number:032x}"
header = ["causal-replica1", A]
receipt_header = ["causal-batches1", "replica", A]


def version(address, author, counter, clock, payload):
    return ["causal-version1", str(address), [author, str(counter)],
            [[who, str(count)] for who, count in sorted(clock.items())], payload]


payload = ["0", "2", "\ufeff雪😀\0".encode().hex()]
first = version(1, A, 1, {A: 1}, payload)
second = version(2, A, 2, {A: 2}, None)
third = version(3, A, 3, {A: 3}, ["2", "3", None])
edits = [["1", [], payload], ["2", [], None]]
owner = [header, ["causal-write1", token(100), [], first],
         ["causal-write1", token(101), [], second]]
receipt = ["causal-batch1", token(100), "0", "0", edits]
receipts = [receipt_header, receipt]
cases = []


def case(name, owner_entries=owner, receipt_entries=receipts, valid=False,
         generation="1", receipt_generation="1", counter="2", versions=(first, second),
         raw_owner=None, raw_receipts=None, **options):
    cases.append(dict(name=name, owner=dict(generation=generation,
                      entries=[wire(v) for v in owner_entries] if raw_owner is None else raw_owner),
                      receipts=dict(generation=receipt_generation,
                      entries=[wire(v) for v in receipt_entries] if raw_receipts is None else raw_receipts),
                      valid=valid, lastCounter=counter, versions=list(versions), options=options))


case("absent", [], [], True, generation="0", receipt_generation="0", counter="0", versions=[])
case("bound-empty-owner-no-batches", [header], [], True,
     receipt_generation="0", counter="0", versions=[])
case("bound-empty-metadata", [header], [receipt_header], True, counter="0", versions=[])
empty = ["causal-batch1", token(200), "0", "0", []]
case("empty-batch", [header], [receipt_header, empty], True, counter="0", versions=[])
case("two-edits-unicode-and-deletion", valid=True)
case("following-standalone-write", owner + [["causal-write1", token(300), [], third]],
     valid=True, generation="2", counter="3", versions=[first, second, third])
case("following-empty-batch", receipt_entries=receipts + [
    ["causal-batch1", token(200), "1", "2", []]], valid=True)
old = version(1, B, 1, {B: 1}, ["2", "3", "6f6c64"])
new = version(1, B, 2, {B: 2}, ["2", "3", "6e6577"])
resolved = version(1, A, 1, {A: 1, B: 2}, None)
case("owner-retains-dominated-observations", [header, ["causal-write1", token(100), [old, new], resolved]],
     [receipt_header, ["causal-batch1", token(100), "0", "0", [["1", [new], None]]]],
     True, counter="1", versions=[resolved])

case("receipt-without-owner", [], generation="0")
case("generation-zero-with-metadata", receipt_generation="0")
case("nonzero-generation-without-metadata", receipt_entries=[])
case("missing-receipt-header", receipt_entries=[receipt])
case("wrong-owner-key", receipt_entries=[["causal-batches1", "other", A], receipt])
case("wrong-author", receipt_entries=[["causal-batches1", "replica", B], receipt])
case("extra-header-field", receipt_entries=[receipt_header + [None], receipt])
case("unknown-header", receipt_entries=[["causal-batches2", "replica", A], receipt])
case("scalar-receipt", receipt_entries=[receipt_header, None])
case("duplicate-batch-token", receipt_entries=receipts + [receipt])
case("overlapping-child-tokens", receipt_entries=receipts + [
    ["causal-batch1", token(101), "0", "1", [["2", [], None]]]])
case("unsupported-receipt-version", receipt_entries=[receipt_header, ["causal-batch2", *receipt[1:]]])
case("extra-receipt-field", receipt_entries=[receipt_header, receipt + [None]])
case("missing-owner-write", owner_entries=[header])
case("owner-counter-does-not-match", receipt_entries=[receipt_header,
    ["causal-batch1", token(100), "0", "1", edits]])
case("empty-start-beyond-owner-counter", receipt_entries=receipts + [
    ["causal-batch1", token(200), "1", "3", []]])
for field, value, name in [(1, "0" * 32, "zero-token"), (1, "F" * 32, "uppercase-token"),
                           (2, "01", "noncanonical-generation"), (2, 0, "numeric-generation"),
                           (2, "", "empty-generation"),
                           (2, "100000000000000000000", "overwidth-generation"),
                           (2, "18446744073709551616", "generation-overflow"),
                           (3, "18446744073709551615", "counter-range-overflow"),
                           (4, None, "non-array-edits"), (4, edits[::-1], "unordered-edits"),
                           (4, [edits[0], edits[0]], "duplicate-address"),
                           (4, [edits[0]] * 65, "too-many-edits")]:
    changed = copy.deepcopy(receipt); changed[field] = value
    case(name, receipt_entries=[receipt_header, changed])
for field, value, name in [(0, "0", "zero-address"), (0, "01", "noncanonical-address"),
                           (0, "18446744073709551616", "address-overflow"),
                           (1, None, "non-array-observations"),
                           (2, ["0", "2", "f"], "invalid-name-hex"),
                           (2, ["0", "2", "00"], "changed-request-payload")]:
    changed = copy.deepcopy(receipt); changed[4][0][field] = value
    case(name, receipt_entries=[receipt_header, changed])
changed = copy.deepcopy(receipt); changed[4] = [None]
case("non-array-edit", receipt_entries=[receipt_header, changed])
changed = copy.deepcopy(receipt); changed[4][0].append(None)
case("extra-edit-field", receipt_entries=[receipt_header, changed])
changed = copy.deepcopy(receipt); changed[1] = token(2 ** 128 - 1)
case("child-token-overflow", receipt_entries=[receipt_header, changed])
case("noncanonical-observed-receipt", [header, ["causal-write1", token(100), [old, new], resolved]],
     [receipt_header, ["causal-batch1", token(100), "0", "0", [["1", [old, new], None]]]])
case("entry-capacity", maxEntries=2, receipt_entries=receipts + [empty])
case("byte-capacity", maxBytes=16)
for name, text in [("trailing-space", wire(receipt) + " "),
                   ("escaped-marker", wire(receipt).replace("causal", "\\u0063ausal", 1)),
                   ("object-frame", "{}"), ("array-depth", "[" * 8 + "]" * 8),
                   ("unbalanced-quote", '"'), ("non-ascii-frame", '"雪"')]:
    case(name, raw_receipts=[wire(receipt_header), text])
parser = argparse.ArgumentParser()
parser.add_argument("output", nargs="?", type=Path, default=root / "docs/protocol/causal-batch-v1.json")
path = parser.parse_args().output
path.write_text(json.dumps(dict(author=A, ownerKey="replica", receiptKey="batches", cases=cases), indent=2) + "\n")
print(f"Generated {len(cases)} independent batch-journal cases in {path}")
