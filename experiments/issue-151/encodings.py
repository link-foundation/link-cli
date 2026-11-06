#!/usr/bin/env python3
"""Compare encodings of triplets (index: type from to) in doublets (issue #151).

Every encoding stores the same 1000+ random triplets (the creates of the parity
test, without its updates and deletes: a root, 10 types, then triplets whose type is any triplet and whose from
and to are any triplet or 0 one time in four) in a doublets store with unique
(source, target) pairs and source / target indexes, the way link-cli's stores
work. It then measures:

- doublets per triplet (markers excluded);
- for Each by type, by from and by to: doublets visited through the indexes,
  per query, averaged over every triplet as the queried part, and how many of
  the visited doublets are not results (noise);
- what a delete of every triplet one by one leaves to collect.

Encodings:

- chosen: type as source with a wrapper. (i: T i) for a node; w = (P f),
  p = (w t), (i: T p); a duplicate gets d = (p i) and (i: T d).
- bare: type as source without a wrapper. (i: T i); p = (f t), (i: T p);
  d = (p i). The pair (f t) has the from-part as its source, so it looks
  like a link of type f.
- pairs: pair of pairs with markers. tw = (Ty T), fw = (P f), p = (fw t),
  (i: tw p); a node (i: tw i); d = (p i). The type is no longer the source.

Run: python3 experiments/issue-151/encodings.py
"""

from collections import defaultdict


class Random:
    """The LCG of the tests (C# and Rust)."""

    def __init__(self, seed):
        self.state = seed

    def next(self, bound):
        self.state = (self.state * 6364136223846793005 + 1442695040888963407) % (
            1 << 64
        )
        return (self.state >> 33) % bound


class Doublets:
    """A doublets store with unique pairs and source / target indexes."""

    def __init__(self):
        self.links = {}
        self.by_pair = {}
        self.sources = defaultdict(set)
        self.targets = defaultdict(set)
        self.next_id = 1
        self.visited = 0

    def create(self, source=None, target=None):
        index = self.next_id
        self.next_id += 1
        source = index if source is None else source
        target = index if target is None else target
        self.set(index, source, target)
        return index

    def set(self, index, source, target):
        if index in self.links:
            old = self.links[index]
            del self.by_pair[old]
            self.sources[old[0]].discard(index)
            self.targets[old[1]].discard(index)
        assert (source, target) not in self.by_pair, "unique pairs"
        self.links[index] = (source, target)
        self.by_pair[(source, target)] = index
        self.sources[source].add(index)
        self.targets[target].add(index)

    def delete(self, index):
        source, target = self.links.pop(index)
        del self.by_pair[(source, target)]
        self.sources[source].discard(index)
        self.targets[target].discard(index)

    def get_or_create(self, source, target):
        found = self.by_pair.get((source, target))
        return found if found is not None else self.create(source, target)

    # Reads that count what they visit.
    def search(self, source, target):
        self.visited += 1
        return self.by_pair.get((source, target))

    def with_source(self, source):
        found = sorted(self.sources.get(source, ()))
        self.visited += len(found)
        return found

    def with_target(self, target):
        found = sorted(self.targets.get(target, ()))
        self.visited += len(found)
        return found


class Encoding:
    """Shared parts: markers, the pair of (from, to), duplicates and reads."""

    name = ""

    def __init__(self):
        self.store = Doublets()
        self.nothing = self.store.create()
        self.pair_marker = self.store.create()
        self.markers = 2

    def enc(self, part):
        return self.nothing if part == 0 else part

    def outer_source(self, type_id):
        return type_id

    def type_of_source(self, source):
        return source

    def pair_of(self, frm, to):
        raise NotImplementedError

    def find_pair(self, frm, to):
        raise NotImplementedError

    def create(self, type_id, frm, to):
        source = self.outer_source(type_id)
        if frm == 0 and to == 0:
            return self.store.create(source, None)
        pair = self.pair_of(frm, to)
        if pair in self.triplets:
            # The pair is a doublet that already is a triplet: (f t) = (t: f t).
            self.collisions += 1
        if (source, pair) not in self.store.by_pair:
            return self.store.create(source, pair)
        index = self.store.create(source, None)
        self.store.set(index, source, self.store.get_or_create(pair, index))
        return index

    def triplets_on_pair(self, pair):
        found = []
        for outer in self.store.with_target(pair):
            found.append(outer)
        for duplicate in self.store.with_source(pair):
            self.store.visited += 1
            found.append(self.store.links[duplicate][1])
        return found

    def by_type(self, type_id):
        source = self.outer_source_read(type_id)
        if source is None:
            return []
        return [i for i in self.store.with_source(source) if self.is_triplet(i)]

    def outer_source_read(self, type_id):
        return type_id


class Chosen(Encoding):
    name = "chosen: (i: T p), p = ((P f) t)"

    def pair_of(self, frm, to):
        wrapper = self.store.get_or_create(self.pair_marker, self.enc(frm))
        return self.store.get_or_create(wrapper, self.enc(to))

    def is_triplet(self, index):
        self.store.visited += 1
        return index in self.triplets

    def by_from(self, frm):
        wrapper = self.store.search(self.pair_marker, frm)
        found = []
        if wrapper is not None:
            for pair in self.store.with_source(wrapper):
                found.extend(self.triplets_on_pair(pair))
        return found

    def by_to(self, to):
        found = []
        for pair in self.store.with_target(to):
            source = self.store.links[pair][0]
            self.store.visited += 1
            if (
                source != self.pair_marker
                and self.store.links.get(source, (None,))[0] == self.pair_marker
            ):
                found.extend(self.triplets_on_pair(pair))
        return found


class Bare(Encoding):
    name = "bare: (i: T p), p = (f t)"

    def pair_of(self, frm, to):
        return self.store.get_or_create(self.enc(frm), self.enc(to))

    def is_triplet(self, index):
        self.store.visited += 1
        return index in self.triplets

    def by_from(self, frm):
        # The pairs of from f are the doublets whose source is f: the same
        # index as the triplets of type f, so both lists are read for either.
        found = []
        for pair in self.store.with_source(frm):
            if pair not in self.triplets:
                found.extend(self.triplets_on_pair(pair))
        return found

    def by_to(self, to):
        found = []
        for pair in self.store.with_target(to):
            self.store.visited += 1
            if pair not in self.triplets:
                found.extend(self.triplets_on_pair(pair))
        return found


class Pairs(Encoding):
    name = "pairs: (i: (Ty T) p), p = ((P f) t)"

    def __init__(self):
        super().__init__()
        self.type_marker = self.store.create()
        self.markers = 3

    def outer_source(self, type_id):
        return self.store.get_or_create(self.type_marker, type_id)

    def outer_source_read(self, type_id):
        return self.store.search(self.type_marker, type_id)

    pair_of = Chosen.pair_of
    is_triplet = Chosen.is_triplet
    by_from = Chosen.by_from
    by_to = Chosen.by_to


def workload():
    """The creates of the parity test (no updates or deletes): (type, from, to) with indices 1.."""
    random = Random(151)
    triplets = [(1, 0, 0)] + [(1, 0, 0)] * 10
    ids = list(range(1, 12))
    while len(triplets) < 1011:
        type_id = ids[random.next(len(ids))]

        def part():
            return 0 if random.next(4) == 0 else ids[random.next(len(ids))]

        frm = part()
        to = part()
        triplets.append((type_id, frm, to))
        ids.append(len(triplets))
    return triplets


def measure(encoding_type, triplets):
    encoding = encoding_type()
    encoding.triplets = set()
    encoding.collisions = 0
    address = {}
    for number, (type_id, frm, to) in enumerate(triplets, start=1):
        # A triplet may use itself only as the root does; parts are earlier triplets.
        if number == 1:
            index = (
                encoding.store.create(None, None)
                if not isinstance(encoding, Pairs)
                else None
            )
            if index is None:
                index = encoding.store.create(None, None)
                source = encoding.outer_source(index)
                encoding.store.set(index, source, index)
        else:
            index = encoding.create(
                address[type_id], address.get(frm, 0), address.get(to, 0)
            )
        address[number] = index
        encoding.triplets.add(index)
    expected = {
        address[n]: (address[t], address.get(f, 0), address.get(o, 0))
        for n, (t, f, o) in enumerate(triplets, start=1)
    }
    doublets = len(encoding.store.links) - encoding.markers
    result = {
        "doublets": doublets,
        "per_triplet": doublets / len(expected),
        "collisions": encoding.collisions,
    }
    for kind, read, part in (
        ("type", encoding.by_type, 0),
        ("from", encoding.by_from, 1),
        ("to", encoding.by_to, 2),
    ):
        visited = results = wrong = 0
        worst = 0
        for queried in expected:
            encoding.store.visited = 0
            found = set(read(queried))
            truth = {i for i, parts in expected.items() if parts[part] == queried}
            if found != truth:
                wrong += 1
            visited += encoding.store.visited
            results += len(truth)
            worst = max(worst, encoding.store.visited - len(truth))
        result[kind] = (
            visited / len(expected),
            (visited - results) / len(expected),
            worst,
        )
        result["wrong"] = result.get("wrong", 0) + wrong
    return result


def main():
    triplets = workload()
    print(
        f"{len(triplets)} triplets, {sum(1 for _, f, t in triplets if f == 0 and t == 0)} of them nodes"
    )
    print()
    print(
        "| encoding | doublets | per triplet | Each by type: visited / noise / worst noise | by from | by to | pairs that are triplets | wrong answers |"
    )
    print("|---|---|---|---|---|---|---|---|")
    for encoding_type in (Chosen, Bare, Pairs):
        r = measure(encoding_type, triplets)
        cells = [
            f"{v:.1f} / {n:.1f} / {w}" for v, n, w in (r["type"], r["from"], r["to"])
        ]
        print(
            f"| {encoding_type.name} | {r['doublets']} | {r['per_triplet']:.2f} | "
            + " | ".join(cells)
            + f" | {r['collisions']} | {r['wrong']} |"
        )


if __name__ == "__main__":
    main()
