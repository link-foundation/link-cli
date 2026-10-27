#!/usr/bin/env python3
"""Finite local-batch design model; deliberately imports no production codec."""
import copy
import itertools
import json
import resource
from dataclasses import dataclass

resource.setrlimit(resource.RLIMIT_AS, (192 * 1024 * 1024,) * 2)
resource.setrlimit(resource.RLIMIT_STACK, (8 * 1024 * 1024,) * 2)
MAX = 2**64 - 1
OWNER, REMOTE = "0" * 31 + "1", "0" * 31 + "2"


class Rejected(Exception):
    pass


class Uncertain(Exception):
    pass


def require(condition):
    if not condition:
        raise Rejected()


@dataclass(frozen=True)
class Version:
    address: int
    author: str
    counter: int
    clock: tuple
    payload: tuple | None


def wire(value):
    require(1 <= value.address <= MAX and 1 <= value.counter <= MAX)
    require(value.author in (OWNER, REMOTE))
    require(value.clock == tuple(sorted(dict(value.clock).items())))
    require(1 <= len(value.clock) <= 2)
    require(dict(value.clock).get(value.author) == value.counter)
    require(all(author in (OWNER, REMOTE) and 1 <= counter <= MAX
                for author, counter in value.clock))
    payload = None
    if value.payload is not None:
        source, target, name = value.payload
        require(0 <= source <= MAX and 0 <= target <= MAX)
        require(name is None or isinstance(name, str) and len(name.encode()) <= 4096)
        payload = [str(source), str(target), None if name is None else name.encode().hex()]
    return ["causal-version1", str(value.address), [value.author, str(value.counter)],
            [[a, str(c)] for a, c in value.clock], payload]


def dominates(left, right):
    return all(dict(left.clock).get(author, 0) >= counter for author, counter in right.clock)


def merge(values):
    unique = {}
    for value in values:
        wire(value)
        require(not unique or next(iter(unique.values())).address == value.address)
        identity = value.author, value.counter
        require(identity not in unique or unique[identity] == value)
        require(all(other.clock != value.clock or other == value for other in unique.values()))
        unique[identity] = value
    return tuple(sorted((value for value in unique.values()
                         if not any(other != value and dominates(other, value)
                                    for other in unique.values())), key=lambda v: (v.author, v.counter)))


def advance(author, counter, address, seen, payload):
    require(counter < MAX)
    require(all(value.address == address for value in seen))
    clock = {}
    for value in merge(seen):
        for name, number in value.clock:
            clock[name] = max(clock.get(name, 0), number)
    require(clock.get(author, 0) <= counter)
    clock[author] = counter + 1
    value = Version(address, author, counter + 1, tuple(sorted(clock.items())), payload)
    wire(value)
    return value


@dataclass(frozen=True)
class Request:
    token: str
    generation: int
    # Strictly increasing positive addresses; at most one effective edit per row.
    edits: tuple


class Store:
    def __init__(self):
        self.generation = 0
        self.counter = 0
        self.rows = {}
        self.history = {}
        self.receipts = {}
        self.journal = []

    def remember(self, values, local=False, update=True):
        for value in values:
            wire(value)
            identity = value.author, value.counter
            require(identity not in self.history or self.history[identity] == value)
            require(local or value.author != OWNER or identity in self.history)
            require(dict(value.clock).get(OWNER, 0) <= self.counter)
            require(all(prior.address != value.address or prior.clock != value.clock or prior == value
                        for prior in self.history.values()))
            self.history[identity] = value
            if update:
                self.rows[value.address] = merge(self.rows.get(value.address, ()) + (value,))

    def receive(self, values):
        candidate = copy.deepcopy(self)
        known = set(candidate.history)
        candidate.remember(values)
        added = tuple(value for value in values if (value.author, value.counter) not in known)
        if added:
            candidate.journal.append(("receive", added))
            candidate.generation += 1
            self.__dict__ = candidate.__dict__

    def publish(self, request, fault=None):
        require(0 <= request.generation <= MAX and 0 <= len(request.edits) <= 3)
        require(len(request.token) == 32 and request.token != "0" * 32)
        require(all(c in "0123456789abcdef" for c in request.token))
        addresses = tuple(edit[0] for edit in request.edits)
        require(addresses == tuple(sorted(set(addresses))))
        for address, seen, payload in request.edits:
            advance(OWNER, 0 if not seen else max(dict(v.clock).get(OWNER, 0) for v in seen),
                    address, seen, payload)
        if request.token in self.receipts:
            previous, result = self.receipts[request.token]
            require(previous == request)
            return result
        require(request.generation == self.generation)
        candidate = copy.deepcopy(self)
        mutations = []
        for address, seen, payload in request.edits:
            candidate.remember(seen, update=False)
            mutation = advance(OWNER, candidate.counter, address, seen, payload)
            candidate.counter = mutation.counter
            candidate.remember((mutation,), local=True)
            mutations.append(mutation)
        result = tuple(mutations)
        candidate.receipts[request.token] = request, result
        candidate.journal.append(("batch", request, result))
        candidate.generation += 1
        if fault == "before":
            raise Uncertain()
        self.__dict__ = candidate.__dict__
        if fault == "after":
            raise Uncertain()
        return result

    def replay(self):
        reopened = Store()
        for entry in self.journal:
            if entry[0] == "receive":
                reopened.receive(entry[1])
            else:
                _, request, expected = entry
                require(reopened.publish(request) == expected)
        return reopened


def token(index):
    return f"{index:032x}"


def rejected_without_effect(store, operation):
    previous = copy.deepcopy(store.__dict__)
    try:
        operation()
    except Rejected:
        pass
    else:
        raise AssertionError("expected a rejected request")
    assert store.__dict__ == previous


scenarios = 0
for count, fault, intervening in itertools.product(range(4), (None, "before", "after"), (False, True)):
    store = Store()
    request = Request(token(1), 0, tuple((address, (), (0, address, "\ufeff雪😀\0"))
                                        for address in range(1, count + 1)))
    prior = copy.deepcopy(store.__dict__)
    try:
        result = store.publish(request, fault)
    except Uncertain:
        assert (store.__dict__ == prior) == (fault == "before")
        result = store.publish(request)
    assert [value.counter for value in result] == list(range(1, count + 1))
    assert len(store.journal) == 1
    if intervening:
        seen = store.rows.get(1, ())
        store.publish(Request(token(2), store.generation, ((1, seen, None),)))
        store.receive((advance(REMOTE, 0, 3, (), (0, 0, None)),))
    before_retry = copy.deepcopy(store.__dict__)
    assert store.publish(request) == result
    assert store.__dict__ == before_retry
    assert store.replay().__dict__ == store.__dict__
    rejected_without_effect(store, lambda: store.publish(Request(token(1), 1, request.edits)))
    rejected_without_effect(store, lambda: store.publish(Request(token(3), 0, request.edits)))
    scenarios += 1

for order in itertools.permutations((1, 2)):
    store = Store()
    requests = {index: Request(token(index), 0, ((1, (), (index, index, None)),
                                                (2, (), (index, 0, None)))) for index in order}
    result = store.publish(requests[order[0]])
    rejected_without_effect(store, lambda: store.publish(requests[order[1]]))
    assert store.counter == 2 and len(store.journal) == 1
    assert [store.rows[address][0] for address in (1, 2)] == list(result)
    assert store.replay().__dict__ == store.__dict__
    scenarios += 1

store = Store()
original = store.publish(Request(token(1), 0, ((1, (), (1, 2, None)),)))[0]
remote = advance(REMOTE, 0, 2, (), (0, 0, None))
store.receive((remote,))
bad = Version(2, REMOTE, 1, remote.clock, None)
# First edit could succeed, but a contradiction in the second rejects the whole batch.
request = Request(token(2), store.generation, ((1, (original,), None), (2, (bad,), None)))
rejected_without_effect(store, lambda: store.publish(request))
assert store.rows[1] == (original,) and store.counter == 1
scenarios += 1

# Global dot progression must not be reconstructed from the last visible row.
store.publish(Request(token(3), store.generation, ((1, (original,), None), (3, (), (0, 0, None)))))
assert store.counter == 3 and store.rows[3][0].counter == 3
assert store.replay().__dict__ == store.__dict__
scenarios += 1

# A whole-request counter preflight must not overflow after publishing its first row.
terminal = Store()
terminal.counter = MAX - 1
request = Request(token(1), 0, ((1, (), None), (2, (), None)))
rejected_without_effect(terminal, lambda: terminal.publish(request))
assert terminal.counter == MAX - 1 and not terminal.journal
scenarios += 1

# Ordering includes tombstones; filtering them first would resurrect a losing value.
live = advance(OWNER, 0, 1, (), (0, 0, None))
erased = advance(REMOTE, 0, 1, (), None)
siblings = merge((live, erased))
assert len(siblings) == 2 and max(siblings, key=lambda v: (v.author, v.counter)).payload is None
assert max(merge((live, erased, live)), key=lambda v: (v.author, v.counter)) == erased
scenarios += 1

print("PR: https://github.com/link-foundation/link-cli/pull/139")
print(f"PASS {scenarios} finite local-batch design scenarios; imports no production implementation.")
print("Includes 24 size/fault/intervening schedules, both stale-session orders, later-row rejection,")
print("whole-batch counter exhaustion, exact retry/replay and concurrent deletion projection.")
print("Bounds: two authors, three rows/edits per batch, finite schedules, 192 MiB address space, 8 MiB stack.")
print("This model is not production parity, raw-wire admission or physical durability evidence.")
