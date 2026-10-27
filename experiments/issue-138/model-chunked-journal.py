#!/usr/bin/env python3
"""Finite decision-order oracle; no production codec or storage implementation.

At most 32 records, eight immutable chunks, 18 operation cuts. SHA-256 is used
only to make this oracle independent of the proposed production address codec.
"""
from copy import deepcopy
from hashlib import sha256
import json


class Cut(Exception):
    pass


def payload(records):
    return json.dumps(records, ensure_ascii=False, separators=(",", ":")).encode()


def chunks(records, count=4, limit=512):
    if not 1 <= count <= 8 or len(records) > 32:
        raise ValueError("admission")
    result = [records[i:i + count] for i in range(0, len(records), count)]
    if len(result) > 8 or any(len(payload(part)) > limit for part in result):
        raise ValueError("admission")
    return result


class Store:
    def __init__(self):
        self.objects = {}
        self.root = (0, ())
        self.written = 0
        self.read = 0

    def view(self):
        result = []
        for key in self.root[1]:
            data = self.objects[key]
            if sha256(payload(data)).hexdigest() != key:
                raise ValueError("corrupt immutable object")
            self.read += len(payload(data))
            result.extend(data)
        return result

    def publish(self, expected, records, cut=None):
        prepared = chunks(records)  # Complete admission precedes every effect.
        trace = 0

        def boundary():
            nonlocal trace
            if trace == cut:
                raise Cut
            trace += 1

        references = []
        for part in prepared:
            key = sha256(payload(part)).hexdigest()
            boundary()
            if key not in self.objects:
                self.objects[key] = deepcopy(part)
                self.written += len(payload(part))
            boundary()
            self.read += len(payload(self.objects[key]))
            if self.objects[key] != part:
                raise ValueError("immutable collision")
            references.append(key)
        boundary()
        if self.root[0] != expected:
            return False
        self.root = (expected + 1, tuple(references))
        boundary()  # Lost reply after the complete atomic publication.
        return True


def main():
    original = [f"row-{i}:雪\0" for i in range(15)]
    following = original + ["following 😀"]
    seed = Store()
    assert seed.publish(0, original)
    cuts = 2 * len(chunks(following)) + 2
    for point in range(cuts):
        store = deepcopy(seed)
        try:
            store.publish(1, following, point)
        except Cut:
            pass
        assert store.view() in [original, following], point
        # Discover before retry; a lost reply can already have committed.
        visible = store.view()
        assert store.publish(store.root[0], visible + ["after recovery"])
        assert store.view() == visible + ["after recovery"]

    stale = deepcopy(seed)
    assert stale.publish(1, following)
    selected = stale.root
    assert not stale.publish(1, ["stale writer"])
    assert stale.root == selected and stale.view() == following
    empty = Store()
    assert empty.publish(0, []) and empty.root == (1, ())
    assert empty.publish(1, ["next"]) and empty.view() == ["next"]
    for records in [["x"] * 33, ["x" * 513]]:
        store = deepcopy(seed)
        before = deepcopy(store.__dict__)
        try:
            store.publish(1, records)
            raise AssertionError("invalid admission accepted")
        except ValueError:
            pass
        assert store.__dict__ == before
    collision = deepcopy(seed)
    before = collision.root
    collision.objects[sha256(payload(["collision"])).hexdigest()] = ["different"]
    before_objects = deepcopy(collision.objects)
    try:
        collision.publish(1, ["collision"])
        raise AssertionError("collision accepted")
    except ValueError:
        pass
    assert collision.root == before and collision.view() == original
    assert collision.objects == before_objects

    measured = deepcopy(seed)
    measured.written = measured.read = 0
    assert measured.publish(1, following)
    new_bytes, staging_reads = measured.written, measured.read
    whole_bytes = len(payload(following))
    assert 0 < new_bytes < whole_bytes
    print(f"PASS {cuts} finite before/after publication cuts; stale, empty, admission and collision controls")
    print(f"MODEL append: whole journal {whole_bytes} bytes; new chunks {new_bytes} bytes; staging reads {staging_reads} bytes")
    print("Finite decision model only; production APIs, codecs, runtime costs and reclamation remain unverified.")


if __name__ == "__main__":
    main()
