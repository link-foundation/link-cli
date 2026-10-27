#!/usr/bin/env python3
"""Finite independent session reference; imports no production implementation."""
import copy
import itertools
import resource

resource.setrlimit(resource.RLIMIT_AS, (192 * 1024 * 1024,) * 2)
resource.setrlimit(resource.RLIMIT_STACK, (8 * 1024 * 1024,) * 2)
INITIAL_ROWS = {1: (0, 2, "old"), 3: (1, 1, None), 4: (0, 2, None)}


class Store:
    def __init__(self):
        self.generation, self.counter, self.calls = 1, 2, 0
        self.rows = copy.deepcopy(INITIAL_ROWS)
        self.receipts = {}

    def publish(self, token, base, edits, fault=None):
        request = base, copy.deepcopy(edits)
        if token in self.receipts:
            original, result = self.receipts[token]
            if original != request:
                raise ValueError("changed retry")
            return copy.deepcopy(result)
        if base != self.generation:
            raise ValueError("stale base")
        if fault == "before":
            raise OSError("unknown reply before publication")
        self.calls += 1
        result = []
        for address, observed, payload in edits:
            assert observed == ({1: ("a1", "b1"), 2: ("b2",)}.get(address, ()))
            self.counter += 1
            result.append((address, self.counter, payload))
            if payload is None:
                self.rows.pop(address, None)
            else:
                self.rows[address] = copy.deepcopy(payload)
        self.generation += 1
        self.receipts[token] = request, copy.deepcopy(result)
        if fault == "after":
            raise OSError("unknown reply after publication")
        return result


class Session:
    def __init__(self, store, allocation=(32, 33)):
        first, last = allocation
        if not 1 <= first <= last <= 2**64 - 1 or any(first <= address <= last for address in (1, 2, 3, 4)):
            raise ValueError("reservation must be valid and unused, including deletions")
        self.store, self.base = store, store.generation
        self.original = copy.deepcopy(store.rows)
        self.rows = copy.deepcopy(self.original)
        self.next, self.last = allocation
        self.state, self.request, self.result = "active", None, None

    def change(self, operation):
        if self.state != "active":
            raise ValueError("session is frozen")
        if operation == "create":
            if self.next > self.last or self.next in self.rows:
                raise ValueError("reservation exhausted or occupied")
            self.rows[self.next] = (0, 2, None)
            self.next += 1
        elif operation == "rename":
            if 1 not in self.rows:
                raise ValueError("missing row")
            self.rows[1] = (*self.rows[1][:2], "new\ufeff\0🙂")
        elif operation == "update":
            if 1 not in self.rows:
                raise ValueError("missing row")
            self.rows[1] = (2, 0, self.rows[1][2])
        elif operation == "delete":
            if 1 not in self.rows:
                raise ValueError("missing row")
            del self.rows[1]
        elif operation == "restore":
            self.rows[1] = self.original[1]
        elif operation == "cancel-create":
            self.rows.pop(32, None)
        else:
            assert operation == "noop"

    def prepare(self, token):
        if self.state != "active":
            raise ValueError("session is frozen")
        edits = [(address, {1: ("a1", "b1"), 2: ("b2",)}.get(address, ()),
                  copy.deepcopy(self.rows.get(address)))
                 for address in sorted(self.original.keys() | self.rows.keys())
                 if self.original.get(address) != self.rows.get(address)]
        # Even an empty request uses the explicit batch receipt contract. It
        # consumes metadata but no author counter; its base is still checked.
        self.request = token, self.base, edits
        self.state = "prepared"

    def commit(self, token, fault=None):
        if self.state == "active":
            self.prepare(token)
        if self.state == "disposed" or self.request[0] != token:
            raise ValueError("invalid completion")
        if self.state == "completed":
            return copy.deepcopy(self.result)
        result = self.store.publish(*self.request, fault)
        self.result, self.state = copy.deepcopy(result), "completed"
        return result

    def dispose(self):
        self.rows.clear()
        self.state = "disposed"


operations = ("create", "rename", "update", "delete", "restore", "cancel-create", "noop")
scenarios = retries = 0
for sequence in itertools.product(operations, repeat=3):
    for fault in (None, "before", "after"):
        store = Store()
        session = Session(store)
        for operation in sequence:
            before = copy.deepcopy(session.rows), session.next
            try:
                session.change(operation)
            except ValueError:
                assert (session.rows, session.next) == before
            assert store.rows == INITIAL_ROWS and store.calls == 0
        expected = copy.deepcopy(session.rows)
        session.prepare("token")
        frozen = copy.deepcopy(session.request)
        try:
            session.change("create")
            raise AssertionError("frozen session changed")
        except ValueError:
            pass
        try:
            session.commit("token", fault)
        except OSError:
            assert session.state == "prepared" and session.request == frozen
            try:
                session.commit("other-token")
                raise AssertionError("changed retry admitted")
            except ValueError:
                pass
            session.commit("token")
            retries += 1
        assert store.rows == expected
        assert store.calls == 1 and store.counter == 2 + len(frozen[2])
        result = session.commit("token")
        store.generation += 1  # Following publication cannot change old receipt.
        result.clear()
        assert session.commit("token") == session.result
        assert store.publish(*frozen) == session.result and store.calls == 1
        session.dispose()
        try:
            session.commit("token")
            raise AssertionError("disposed session completed")
        except ValueError:
            pass
        scenarios += 1

store = Store()
session = Session(store)
session.change("rename")
store.generation += 1
before = copy.deepcopy(store.__dict__)
try:
    session.commit("stale")
    raise AssertionError("stale base published")
except ValueError:
    pass
assert store.__dict__ == before and session.state == "prepared"
for fault in ("before", "after"):
    store = Store()
    session = Session(store)
    session.change("delete")
    session.prepare("disposed-unknown")
    request = copy.deepcopy(session.request)
    try:
        session.commit("disposed-unknown", fault)
    except OSError:
        pass
    published = copy.deepcopy(store.rows)
    session.dispose()
    assert store.rows == published  # Cleanup cannot undo unknown publication.
    store.publish(*request)
    assert 1 not in store.rows and store.rows[3] == (1, 1, None)
    assert store.rows[4] == (0, 2, None) and store.calls == 1
for allocation in ((0, 1), (10, 9), (2**64, 2**64), (1, 1), (2, 2), (3, 4)):
    try:
        Session(Store(), allocation)
        raise AssertionError("invalid or occupied reservation admitted")
    except ValueError:
        pass
session = Session(Store(), (2**64 - 1, 2**64 - 1))
session.change("create")
before = copy.deepcopy(session.rows)
try:
    session.change("create")
    raise AssertionError("u64 reservation overflow")
except ValueError:
    pass
assert session.rows == before
print(f"PASS {scenarios} finite session sequences; {retries} exact before/after-reply retries.")
print("PASS private writes, frozen requests, complete sibling observations, reserved u64 allocation, duplicate pairs, non-cascading deletion, final diffs, empty receipts, stale bases and disposal after unknown publication.")
print("Independent design evidence only; no ordinary causal session production API is verified.")
