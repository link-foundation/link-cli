#!/usr/bin/env python3
"""Finite independent oracle for retained siblings and deletion causality.

This tests a design assumption, not production anti-entropy or durability.
All permutations are bounded: five events, two authors and at most 120 orders.
"""
from itertools import permutations, product


def dominates(left, right):
    return all(a >= b for a, b in zip(left, right))


def merge(left, right):
    unique = set(left) | set(right)
    for clock, value in unique:
        assert len(clock) == 2 and all(0 <= counter <= 3 for counter in clock)
        assert all(other_value == value for other_clock, other_value in unique
                   if other_clock == clock), "one causal identity has contradictory data"
    return frozenset((clock, value) for clock, value in unique
                     if not any(other != clock and dominates(other, clock)
                                for other, _ in unique))


def resolve(versions, author, value):
    clock = [max(item[0][n] for item in versions) for n in range(2)]
    clock[author] += 1
    return frozenset({(tuple(clock), value)})


initial = frozenset({((1, 0), "original")})
update = frozenset({((2, 0), "updated")})
deletion = frozenset({((1, 1), None)})
siblings = merge(update, deletion)
assert len(siblings) == 2
assert merge(initial, deletion) == deletion
assert merge(siblings, initial) == siblings

# Forgetting the tombstone resurrects the old value when its replica returns.
assert merge(frozenset(), initial) == initial
assert merge(deletion, initial) == deletion

resolved = resolve(siblings, 0, "application resolution")
assert next(iter(resolved))[0] == (3, 1)
assert merge(resolved, siblings) == resolved

events = (initial, update, deletion, siblings, resolved)
for order in permutations(events):
    result = frozenset()
    for event in order:
        result = merge(result, event)
    assert result == resolved

states = (frozenset(), *events)
for a, b, c in product(states, repeat=3):
    assert merge(a, a) == a
    assert merge(a, b) == merge(b, a)
    assert merge(merge(a, b), c) == merge(a, merge(b, c))

try:
    merge(initial, frozenset({((1, 0), "contradictory")}))
except AssertionError as error:
    assert "contradictory" in str(error)
else:
    raise AssertionError("contradictory causal identity accepted")

print("PASS 120 finite event orders and 216 state triples; merge is idempotent, commutative and associative.")
print("PASS concurrent update/delete stay as siblings; observed resolution dominates both; retained deletion prevents resurrection.")
print("PASS equal causal identities with contradictory payloads reject before reconciliation.")
print("Model only: production native/C#/WASM anti-entropy, persistent counters, bounded Merkle transfer and crash recovery remain unimplemented.")
