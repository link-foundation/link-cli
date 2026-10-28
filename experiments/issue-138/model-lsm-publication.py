#!/usr/bin/env python3
"""Finite independent publication-order oracle; no production codec or I/O.

This models decisions, not a hardware durability proof or an implemented LSM.
Run before implementing the LSM composition, then reuse its cases as contracts.
"""
from copy import deepcopy
from dataclasses import dataclass, field


@dataclass
class Image:
    # The selected WAL checkpoint includes a manifest and bounded buffer.
    generation: int = 5
    covered: int = 2
    last_id: int = 1
    manifest: list[int] = field(default_factory=lambda: [1])
    buffer: dict = field(default_factory=lambda: {
        "a": (3, ["new"]), "deleted": (4, None), "empty": (5, [])})
    # Only fully synchronized and sealed tables may be selected.
    tables: dict = field(default_factory=lambda: {
        1: {"sealed": True, "rows": {
            "a": (1, ["old"]), "deleted": (2, ["obsolete"])}}})


def read(image):
    if not 0 <= image.covered <= image.generation <= (1 << 64) - 1:
        raise ValueError("invalid generation counters")
    if len(set(image.manifest)) != len(image.manifest):
        raise ValueError("duplicate selected ID")
    selected = {}
    for identity in image.manifest:
        if not 0 < identity <= image.last_id <= (1 << 64) - 1:
            raise ValueError("unreserved selected ID")
        table = image.tables.get(identity)
        if table is None or not table["sealed"]:
            raise ValueError("selected table is absent or incomplete")
        for key, value in table["rows"].items():
            if not 0 < value[0] <= image.covered:
                raise ValueError("table exceeds covered generation")
            previous = selected.get(key)
            if previous is not None and previous[0] == value[0] and previous != value:
                raise ValueError("one generation describes inconsistent values")
            if previous is None or previous[0] < value[0]:
                selected[key] = deepcopy(value)
    for key, value in image.buffer.items():
        if not image.covered < value[0] <= image.generation:
            raise ValueError("buffer generation is not after coverage")
        previous = selected.get(key)
        if previous is not None and previous[0] >= value[0]:
            raise ValueError("buffer must follow the covered table generations")
        selected[key] = deepcopy(value)
    if max((value[0] for value in selected.values()), default=0) != image.generation:
        raise ValueError("global generation does not describe retained state")
    return selected


class Interrupted(Exception):
    pass


def compact(durable, cut=None):
    """Every operation has distinct before/after cuts; writes can be durable.

    A table ID is consumed by a durable reservation even if creation fails.
    The final checkpoint is the only publication decision. The old table is
    retired only after this decision is durably selected.
    """
    desired = read(durable)
    if durable.last_id == (1 << 64) - 1:
        raise ValueError("table ID exhaustion")
    identity = durable.last_id + 1
    events = []

    def operation(name, body):
        events.append(name + ":before")
        if cut == len(events) - 1:
            raise Interrupted
        body()
        events.append(name + ":after")
        if cut == len(events) - 1:
            raise Interrupted

    operation("reserve-sync", lambda: setattr(durable, "last_id", identity))
    operation("create", lambda: durable.tables.__setitem__(identity, {
        "sealed": False, "rows": {}}))
    operation("data-sync", lambda: durable.tables[identity].update(rows=deepcopy(desired)))
    operation("seal-sync", lambda: durable.tables[identity].update(sealed=True))

    def publish():
        durable.manifest = [identity]
        durable.buffer = {}
        durable.covered = durable.generation

    operation("manifest-checkpoint-sync", publish)
    operation("retire-old", lambda: durable.tables.pop(1))
    return events


def main():
    original = Image()
    desired = read(original)
    events = compact(deepcopy(original))
    assert len(events) == 12
    for point, event in enumerate(events):
        durable = deepcopy(original)
        try:
            compact(durable, point)
        except Interrupted:
            pass
        else:
            raise AssertionError("expected interruption")
        assert read(durable) == desired, event
        assert desired["deleted"] == (4, None)
        assert desired["empty"] == (5, [])
        assert durable.last_id >= max(durable.tables), event
        # A following reservation never reuses an interrupted physical name.
        next_identity = durable.last_id + 1
        assert next_identity not in durable.tables, event
        durable.last_id = next_identity
        durable.tables[next_identity] = {"sealed": True, "rows": deepcopy(desired)}
        durable.manifest, durable.buffer = [next_identity], {}
        durable.covered = durable.generation
        assert read(durable) == desired, event
        print(f"PASS cut{point:02}: {event}; exact values/tombstone/empty; next ID {next_identity}")

    for damage in ("missing", "unsealed"):
        durable = deepcopy(original)
        compact(durable)
        selected = durable.manifest[0]
        if damage == "missing":
            durable.tables.pop(selected)
        else:
            durable.tables[selected]["sealed"] = False
        try:
            read(durable)
        except ValueError:
            print(f"PASS selected {damage}: error, no obsolete fallback")
        else:
            raise AssertionError("selected damage must fail")
    invalid = (
        ("table generation", lambda image: setattr(image, "covered", 1)),
        ("buffer generation", lambda image: setattr(image, "covered", 3)),
        ("global generation", lambda image: setattr(image, "generation", 6)),
        ("unreserved ID", lambda image: setattr(image, "last_id", 0)),
        ("duplicate ID", lambda image: image.manifest.append(1)),
    )
    for name, damage in invalid:
        image = deepcopy(original)
        damage(image)
        try:
            read(image)
        except ValueError:
            print(f"PASS invalid {name}: reject")
        else:
            raise AssertionError(name)
    image = deepcopy(original)
    image.last_id = (1 << 64) - 2
    compact(image)
    assert image.last_id == (1 << 64) - 1 and read(image) == desired
    unchanged = deepcopy(image)
    try:
        compact(image)
    except ValueError as error:
        assert str(error) == "table ID exhaustion" and image == unchanged
    else:
        raise AssertionError("exhaustion must reject before mutation")
    print("PASS last u64 table ID; following reservation rejects without mutation")
    print("PASS independent finite LSM publication model: 12 cuts, two damage and six counter controls")


if __name__ == "__main__":
    main()
