#!/usr/bin/env python3
"""Finite independent semantics for a proposed ordinary causal-links view.

This imports no production code and is not evidence that the session API exists.
"""
from dataclasses import dataclass
from itertools import combinations, permutations
import json
from pathlib import Path
import resource
import sys

resource.setrlimit(resource.RLIMIT_AS, (192 * 1024 * 1024,) * 2)
resource.setrlimit(resource.RLIMIT_STACK, (8 * 1024 * 1024,) * 2)


@dataclass(frozen=True)
class Version:
    address: int
    author: int
    counter: int
    clock: tuple
    payload: tuple | None

    @property
    def dot(self):
        return self.author, self.counter


def frontier(values):
    """Retain deletion markers and detect contradictions before dominance."""
    by_dot, by_clock = {}, {}
    for value in values:
        assert value.address > 0 and value.counter > 0
        assert dict(value.clock)[value.author] == value.counter
        if value.dot in by_dot and by_dot[value.dot] != value:
            raise ValueError("dot contradiction")
        key = value.address, value.clock
        if key in by_clock and by_clock[key] != value:
            raise ValueError("clock contradiction")
        by_dot[value.dot] = by_clock[key] = value
    unique = list(by_dot.values())
    return tuple(sorted((value for value in unique if not any(
        other != value and other.address == value.address and
        all(dict(other.clock).get(author, 0) >= counter for author, counter in value.clock)
        for other in unique)), key=lambda value: (value.address, value.dot)))


def project(values, policy):
    if policy not in ("reject", "max-dot"):
        raise ValueError("explicit policy required")
    retained = frontier(values)
    groups = {}
    for value in retained:
        groups.setdefault(value.address, []).append(value)
    selected, visible = [], []
    for address, siblings in sorted(groups.items()):
        if policy == "reject" and len(siblings) != 1:
            raise ValueError("concurrent siblings")
        chosen = max(siblings, key=lambda value: value.dot)
        selected.append(chosen)
        if chosen.payload is not None:
            visible.append((address, chosen.payload))
    return retained, tuple(selected), tuple(visible)


def outcome(values, policy):
    try:
        return project(values, policy)
    except ValueError as error:
        return str(error)


live = Version(1, 1, 1, ((1, 1),), (2, 3, "live\ufeff雪😀\0"))
deleted = Version(1, 2, 1, ((2, 1),), None)
pool = (
    live, deleted,
    Version(1, 1, 2, ((1, 2),), (3, 2, "following")),
    Version(2, 1, 3, ((1, 3),), (1, 1, "second")),
    Version(2, 2, 2, ((2, 2),), None),
    Version(2, 2, 3, ((2, 3),), (1, 2, "recreated")),
)
assert project((live, deleted), "max-dot")[2] == ()
assert outcome((live, deleted), "reject") == "concurrent siblings"
# Preserve the before control: filtering tombstones first resurrects a row.
wrong = project(tuple(v for v in (live, deleted) if v.payload is not None), "max-dot")[2]
assert wrong == ((1, live.payload),)

scenarios = schedules = 0
fixtures = []


def wire(value):
    payload = None if value.payload is None else [str(value.payload[0]), str(value.payload[1]),
                                                value.payload[2].encode("utf-8").hex()]
    return ["causal-version1", str(value.address), [f"{value.author:032x}", str(value.counter)],
            [[f"{author:032x}", str(counter)] for author, counter in value.clock], payload]


for size in range(len(pool) + 1):
    for values in combinations(pool, size):
        for policy in ("reject", "max-dot"):
            expected = outcome(values, policy)
            scenarios += 1
            case = {"name": f"subset-{scenarios}-{policy}", "policy": policy,
                    "input": [wire(value) for value in values], "valid": isinstance(expected, tuple)}
            if isinstance(expected, tuple):
                case.update(retained=[wire(value) for value in expected[0]],
                            selected=[wire(value) for value in expected[1]],
                            rows=[[str(address), str(payload[0]), str(payload[1]), payload[2]]
                                  for address, payload in expected[2]])
            else:
                case["error"] = "conflict"
            fixtures.append(case)
            for order in permutations(values):
                assert outcome(order, policy) == expected
                assert outcome(order + order, policy) == expected
                schedules += 1
            if isinstance(expected, tuple):
                retained, chosen, visible = expected
                assert retained == frontier(retained)
                assert chosen == tuple(sorted(chosen, key=lambda value: value.address))
                assert all(value in retained for value in chosen)
                assert tuple((value.address, value.payload) for value in chosen if value.payload is not None) == visible
                # Selection never removes siblings from the retained evidence.
                assert frontier(values) == retained

forged = Version(2, live.author, live.counter, live.clock, live.payload)
assert outcome((live, forged), "max-dot") == "dot contradiction"
dominated_forgery = Version(live.address, live.author, live.counter, live.clock, None)
assert outcome((live, pool[2], dominated_forgery), "max-dot") == "dot contradiction"
assert outcome((), "implicit") == "explicit policy required"
equal_clock_live = Version(1, 1, 1, ((1, 1), (2, 1)), live.payload)
equal_clock_deleted = Version(1, 2, 1, ((1, 1), (2, 1)), None)
resolved_clock = Version(1, 1, 2, ((1, 2), (2, 1)), None)
assert outcome((equal_clock_live, equal_clock_deleted), "max-dot") == "clock contradiction"
assert outcome((equal_clock_live, resolved_clock, equal_clock_deleted), "max-dot") == "clock contradiction"
fixtures.extend([
    {"name": "cross-address-dot-contradiction", "policy": "max-dot", "input": [wire(live), wire(forged)],
     "valid": False, "error": "invalid"},
    {"name": "dominated-dot-contradiction", "policy": "max-dot",
     "input": [wire(live), wire(pool[2]), wire(dominated_forgery)], "valid": False, "error": "invalid"},
    {"name": "equal-clock-different-dot-contradiction", "policy": "max-dot",
     "input": [wire(equal_clock_live), wire(equal_clock_deleted)], "valid": False, "error": "invalid"},
    {"name": "dominated-equal-clock-different-dot-contradiction", "policy": "max-dot",
     "input": [wire(equal_clock_live), wire(resolved_clock), wire(equal_clock_deleted)],
     "valid": False, "error": "invalid"},
])
if len(sys.argv) != 1:
    assert len(sys.argv) == 3 and sys.argv[1] == "--vectors"
    Path(sys.argv[2]).write_text(json.dumps({"format": "causal-projection-v1", "cases": fixtures},
                                          ensure_ascii=True, indent=2) + "\n")
    print(f"Generated {len(fixtures)} independent semantic vectors.")
print("PR: https://github.com/link-foundation/link-cli/pull/139")
print(f"PASS {scenarios} finite projection scenarios, {schedules} input permutation schedules and duplicate replays.")
print("Before control: filtering deletion markers before selection resurrects a row; selection before visibility preserves deletion.")
print("Two explicit policies; complete retained sibling evidence; dominated/cross-address dot contradictions reject.")
print("Bounds: six versions, two authors, two addresses; 192 MiB address space and 8 MiB stack.")
print("Independent design model only. Ordinary causal-links/session production APIs remain unfinished.")
