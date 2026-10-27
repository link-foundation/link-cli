#!/usr/bin/env python3
"""Finite chunk-reference/epoch model, independent of production codecs.

Every root has two immutable chunks. Explore up to two publishers and two
collectors; a negative control reuses a chunk from an earlier epoch. This model
does not establish production or physical-durability correctness.
"""
from dataclasses import dataclass, replace
import resource

resource.setrlimit(resource.RLIMIT_AS, (192 * 1024 * 1024,) * 2)
MAX_STATES = 150_000
DESIRED = (("A", "B"), ("X", "C"))


@dataclass(frozen=True, slots=True)
class State:
    epoch: int
    revision: int
    root: tuple
    objects: frozenset
    writers: tuple
    collectors: tuple


def successors(state, reuse_old_epoch, fence_writers):
    for index, writer in enumerate(state.writers):
        phase, epoch, revision, references = writer
        writers = list(state.writers)
        changes = {}
        if phase == 0:
            writers[index] = (1, state.epoch, state.revision, ())
            label = f"writer{index}: snapshot epoch={state.epoch} revision={state.revision}"
        elif phase in (1, 2):
            content = DESIRED[index][phase - 1]
            key = (epoch, content)
            if reuse_old_epoch:
                key = next((key for key in sorted(state.objects) if key[1] == content), key)
            writers[index] = (phase + 1, epoch, revision, references + (key,))
            changes["objects"] = state.objects | {key}
            label = f"writer{index}: stage/verify chunk {key}"
        elif phase == 3:
            accepted = revision == state.revision and (not fence_writers or epoch == state.epoch)
            if accepted:
                changes.update(root=references, revision=state.revision + 1)
            writers[index] = (4, None, None, ())
            label = f"writer{index}: publish complete root {references} accepted={accepted}"
        else:
            continue
        yield label, replace(state, writers=tuple(writers), **changes)
    for index, collector in enumerate(state.collectors):
        phase, fence, protected, candidates = collector
        collectors = list(state.collectors)
        changes = {}
        if phase == 0:
            fence = state.epoch + 1
            changes["epoch"] = fence
            collectors[index] = (1, fence, (), ())
            label = f"collector{index}: advance epoch={fence}"
        elif phase == 1:
            collectors[index] = (2, fence, state.root, ())
            label = f"collector{index}: protect EVERY root chunk {state.root}"
        elif phase == 2:
            candidates = tuple(sorted(key for key in state.objects if key[0] < fence and key not in protected))
            collectors[index] = (3, fence, protected, candidates)
            label = f"collector{index}: list candidates={candidates}"
        elif phase == 3 and candidates:
            key, *remaining = candidates
            changes["objects"] = state.objects - {key}
            collectors[index] = (3, fence, protected, tuple(remaining))
            label = f"collector{index}: delete {key}"
        elif phase == 3:
            collectors[index] = (4, None, (), ())
            label = f"collector{index}: finish"
        else:
            continue
        yield label, replace(state, collectors=tuple(collectors), **changes)


def explore(writers, collectors, reuse_old_epoch=False, fence_writers=True):
    root = ((0, "A"), (0, "X"))
    initial = State(0, 0, root, frozenset(root), ((0, None, None, ()),) * writers,
                    ((0, None, (), ()),) * collectors)
    seen = set()
    path = []
    terminals = 0

    def visit(state):
        nonlocal terminals
        if not set(state.root) <= state.objects:
            return path.copy()
        if state in seen:
            return None
        if len(seen) >= MAX_STATES:
            raise RuntimeError("finite state budget exhausted; exploration incomplete")
        seen.add(state)
        progressed = False
        for label, after in successors(state, reuse_old_epoch, fence_writers):
            progressed = True
            path.append(label)
            failure = visit(after)
            if failure is not None:
                return failure
            path.pop()
        if not progressed:
            terminals += 1
        return None

    failure = visit(initial)
    return len(seen), terminals, failure


def main():
    for label, reuse, fenced in [("reuse old-epoch chunks", True, True), ("omit epoch CAS", False, False)]:
        count, _, failure = explore(2, 1, reuse, fenced)
        assert failure is not None, f"unsafe {label} control must lose a live chunk"
        print(f"NEGATIVE {label}: live chunk lost after {count} states")
        for step in failure:
            print("  " + step)
    for writers, collectors in [(2, 1), (1, 2), (2, 2)]:
        count, terminals, failure = explore(writers, collectors)
        assert failure is None and terminals > 0, failure
        print(f"PASS current-epoch chunks writers={writers} collectors={collectors}: {count} complete states, {terminals} terminals")
    print("Finite design model only; exact-generation deletion, owned-reader refresh, provider faults and production parity need their own controls.")


if __name__ == "__main__":
    main()
