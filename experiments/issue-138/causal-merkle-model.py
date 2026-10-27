#!/usr/bin/env python3
"""Independent finite snapshot/proof oracle; imports no production implementation."""
import copy
import hashlib
import json
from pathlib import Path
import resource

resource.setrlimit(resource.RLIMIT_AS, (192 * 1024 * 1024,) * 2)
resource.setrlimit(resource.RLIMIT_STACK, (8 * 1024 * 1024,) * 2)
DOMAIN = b"causal-merkle1\0"
MAX = 2**64 - 1


def digest(data):
    return hashlib.sha256(data).digest()


def branch(depth, children):
    return digest(DOMAIN + b"N" + bytes([depth]) + b"".join(children))


EMPTY = {16: digest(DOMAIN + b"E")}
for level in range(15, -1, -1):
    EMPTY[level] = branch(level, [EMPTY[level + 1]] * 16)


def leaf(address, versions):
    if not versions:
        return EMPTY[16]
    wire = [json.dumps(value, separators=(",", ":")).encode("ascii") for value in versions]
    return digest(DOMAIN + b"L" + address.to_bytes(8, "big") + len(wire).to_bytes(2, "big") +
                  b"".join(len(value).to_bytes(4, "big") + value for value in wire))


class Snapshot:
    def __init__(self, rows):
        self.rows = copy.deepcopy(rows)
        self.nodes = {(16, key): leaf(key, values) for key, values in self.rows.items()}
        for depth in range(15, -1, -1):
            prefixes = {prefix >> 4 for (level, prefix) in self.nodes if level == depth + 1}
            for prefix in prefixes:
                self.nodes[depth, prefix] = branch(depth, self.children(depth, prefix))
        self.root = self.hash(0, 0)

    def hash(self, depth, prefix):
        return self.nodes.get((depth, prefix), EMPTY[depth])

    def children(self, depth, prefix):
        return [self.hash(depth + 1, prefix * 16 + digit) for digit in range(16)]

    def proof(self, address):
        assert 1 <= address <= MAX
        path = []
        for depth in range(16):
            prefix = address >> ((16 - depth) * 4)
            digit = (address >> ((15 - depth) * 4)) & 15
            path.append([value for index, value in enumerate(self.children(depth, prefix)) if index != digit])
        return copy.deepcopy(self.rows.get(address, [])), path


def verify(root, address, versions, path):
    if not 1 <= address <= MAX or len(root) != 32 or len(path) != 16 or len(versions) > 64:
        return False
    if any(int(value[1]) != address for value in versions):
        return False
    current = leaf(address, versions)
    for depth in range(15, -1, -1):
        siblings = path[depth]
        if len(siblings) != 15 or any(len(value) != 32 for value in siblings):
            return False
        digit = (address >> ((15 - depth) * 4)) & 15
        children = list(siblings); children.insert(digit, current)
        current = branch(depth, children)
    return current == root


def differences(left, right):
    result, visited = [], 0

    def compare(depth, prefix):
        nonlocal visited
        visited += 1
        if left.hash(depth, prefix) == right.hash(depth, prefix):
            return
        if depth == 16:
            result.append(prefix)
        else:
            for digit in range(16):
                compare(depth + 1, prefix * 16 + digit)

    compare(0, 0)
    return result, visited


def version(address, author=1, deletion=False):
    # Dots identify operations across all rows. Distinct delete authors also
    # avoid inventing contradictory payloads under a live value's identity.
    if deletion:
        author += 2
    identity = f"{author:032x}"
    counter = str(address)
    return ["causal-version1", str(address), [identity, counter], [[identity, counter]],
            None if deletion else ["0", str(MAX), "efbbbf" + "雪😀\0".encode().hex()]]


keys = [1, 2, 15, 16, 2**32, MAX]
states = [{}, {1: [version(1)]}, {1: [version(1, deletion=True)]},
          {1: [version(1), version(1, 2, True)]}, {2: [version(2)]},
          {key: [version(key)] for key in keys}, {MAX: [version(MAX)]},
          {key: [version(key, deletion=True)] for key in keys}]
snapshots = [Snapshot(state) for state in states]
comparisons, checks = 0, 0
for left in snapshots:
    for right in snapshots:
        expected = sorted(key for key in set(left.rows) | set(right.rows) if left.rows.get(key) != right.rows.get(key))
        actual, visited = differences(left, right)
        assert actual == expected
        assert visited <= 1 + 16 * 16 * len(expected)
        comparisons += 1
    for key in keys:
        values, path = left.proof(key)
        assert verify(left.root, key, values, path)
        assert not verify(left.root, 0, values, path)
        assert not verify(left.root, MAX + 1, values, path)
        assert not verify(b"\0" * 32, key, values, path)
        assert not verify(left.root, key, values, path[:-1])
        corrupt = copy.deepcopy(path); corrupt[0][0] = b"\0" * 32
        assert not verify(left.root, key, values, corrupt)
        mixed = snapshots[(snapshots.index(left) + 1) % len(snapshots)]
        other_values, other_path = mixed.proof(key)
        if mixed.root != left.root:
            assert not verify(left.root, key, other_values, other_path)
        if values:
            changed = copy.deepcopy(values); changed[0][4] = ["1", "1", "666f72676564"]
            assert not verify(left.root, key, changed, path)
        checks += 1
original = {1: [version(1)]}; frozen = Snapshot(original)
original[1][0][4] = None; original[MAX] = [version(MAX)]
assert frozen.rows == states[1] and frozen.root == snapshots[1].root
vectors = [{"rows": state, "root": snapshot.root.hex()} for state, snapshot in zip(states, snapshots)]
Path(__file__).with_name("causal-merkle-model-vectors.json").write_text(json.dumps(vectors, indent=2) + "\n")
print(f"PASS independent Merkle model: {comparisons} snapshot pairs, {checks} leaf/proof groups; depth 16, fanout 16, six finite addresses.")
print("PASS differing-address oracle, changed leaf/root/path, mixed snapshots, malformed depth, address bounds and frozen ownership controls.")
print("Proof sibling payload is 16 * 15 * 32 = 7680 raw bytes. Empty equal roots visit one node and transfer no leaves.")
print("Model evidence only: canonical protocol admission, production parity, bounded transport/publication, process faults and safe reclamation remain unfinished.")
