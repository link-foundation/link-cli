#!/usr/bin/env python3
"""Independent finite local-journal cases; imports no production codec."""
import copy
import json
from pathlib import Path

A, B = (f"{n:032x}" for n in (1, 2))
wire = lambda value: json.dumps(value, separators=(",", ":"), ensure_ascii=True)
token = lambda n: f"{n:032x}"


def version(address, author, counter, clock, name=None, deletion=False):
    row = None if deletion else ["2", "3", None if name is None else name.encode().hex()]
    return ["causal-version1", str(address), [author, str(counter)],
            [[who, str(count)] for who, count in sorted(clock.items())], row]


initial = version(1, A, 1, {A: 1}, "initial")
deleted = version(1, B, 1, {A: 1, B: 1}, deletion=True)
resolved = version(1, A, 2, {A: 2, B: 1}, "resolved")
other = version(2, A, 3, {A: 3}, deletion=True)
header = ["causal-replica1", A]
w1 = ["causal-write1", token(1), [], initial]
r1 = ["causal-receive1", [deleted]]
w2 = ["causal-write1", token(2), [deleted], resolved]
w3 = ["causal-write1", token(3), [], other]
full = [header, w1, r1, w2, w3]
cases = []


def case(name, entries, valid=False, counter=0, versions=(), generation="1", **options):
    cases.append(dict(name=name, entries=[wire(e) for e in entries], generation=generation,
                      valid=valid, lastCounter=str(counter), versions=list(versions), options=options))


case("absent", [], True, generation="0")
case("bound-empty-owner", [header], True)
case("first-write", [header, w1], True, 1, [initial])
case("observed-deletion", [header, w1, r1], True, 1, [deleted])
case("resolution-and-global-counter", full, True, 3, [resolved, other])
case("generation-zero-with-state", [header], generation="0")
case("nonzero-generation-without-header", [])
case("wrong-author", [["causal-replica1", B]])
case("unsupported-header", [["causal-replica2", A]])
case("missing-header", [w1])
case("unknown-event", [header, ["other"]])
case("scalar-event", [header, 1])
case("empty-receive", [header, ["causal-receive1", []]])
case("reused-token", [header, w1, w1])
case("invalid-token", [header, ["causal-write1", "", [], initial]])
jump = version(1, A, 2, {A: 2}, "initial")
case("local-counter-jump", [header, ["causal-write1", token(1), [], jump]])
case("foreign-unknown-local-dot", [header, ["causal-receive1", [initial]]])
future = version(1, B, 1, {A: 3, B: 1}, "future")
case("foreign-future-local-context", [header, ["causal-receive1", [future]]])
forged = copy.deepcopy(deleted); forged[4] = ["2", "3", "666f72676564"]
case("old-equivocation-after-resolution", full + [["causal-receive1", [forged]]])
wrong_address = copy.deepcopy(deleted); wrong_address[1] = "2"
case("author-dot-reused-on-another-address", full + [["causal-receive1", [wrong_address]]])
case("duplicate-receive-dot", [header, w1, ["causal-receive1", [deleted, deleted]]])
case("entry-bound", full, maxEntries=2)
case("byte-bound", full, maxBytes=16)
case("extra-write-field", [header, w1 + [None]])
case("noncanonical-observed-duplicates", [header, w1, r1, ["causal-write1", token(2), [deleted, deleted], resolved]])
case("foreign-local-payload-in-observed-context", [header, ["causal-write1", token(1), [initial], resolved]])

# Dominated observations still bind historical operation identities.
old_remote = version(1, B, 1, {B: 1}, "old")
new_remote = version(1, B, 2, {B: 2}, "new")
local_resolution = version(1, A, 1, {A: 1, B: 2}, "resolution")
observed = [header, ["causal-write1", token(1), [old_remote, new_remote], local_resolution]]
case("dominated-observation-evidence", observed, True, 1, [local_resolution])
old_forged = copy.deepcopy(old_remote); old_forged[4] = ["2", "3", "666f72676564"]
case("dominated-observation-equivocation-after-reopen", observed + [["causal-receive1", [old_forged]]])
case("dominated-observation-unsorted", [header, ["causal-write1", token(1), [new_remote, old_remote], local_resolution]])

path = Path(__file__).resolve().parents[2] / "docs/protocol/causal-replica-v1.json"
path.write_text(json.dumps(dict(author=A, cases=cases), indent=2) + "\n")
print(f"Generated {len(cases)} independent local-journal cases in {path}")
