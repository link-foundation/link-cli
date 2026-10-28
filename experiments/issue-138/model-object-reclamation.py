#!/usr/bin/env python3
"""Finite design experiment, not a production acceptance test.

Explore epoch-fenced staging/publication, snapshot collection and owned reads.
A required negative control removes epoch comparison and must lose a live object.
The model excludes provider faults and does not prove the production implementation.
"""
from dataclasses import dataclass, replace
import resource

resource.setrlimit(resource.RLIMIT_AS, (192 * 1024 * 1024,) * 2)
MAX_STATES = 100_000


@dataclass(frozen=True, slots=True)
class State:
    epoch: int
    pointer: tuple
    objects: frozenset
    writers: tuple
    collectors: tuple
    reader: tuple = (0, None)


def successors(s, fenced, epoch_keys):
    for i, w in enumerate(s.writers):
        phase, epoch, expected, key = w
        writers = list(s.writers)
        changes = {}
        if phase == 0:
            writers[i] = (1, s.epoch, s.pointer, (s.epoch if epoch_keys else 0, 'BC'[i]))
            label = f'writer{i}: snapshot epoch={s.epoch} pointer={s.pointer}'
        elif phase == 1:
            writers[i] = (2, epoch, expected, key)
            changes['objects'] = s.objects | {key}
            label = f'writer{i}: stage {key}'
        elif phase == 2:
            writers[i] = (3, None, None, None)
            ok = s.pointer == expected and (not fenced or s.epoch == epoch)
            if ok:
                changes['pointer'] = key
            label = f'writer{i}: publish {key} accepted={ok}'
        else:
            continue
        yield label, replace(s, writers=tuple(writers), **changes)
    for i, c in enumerate(s.collectors):
        phase, epoch, snapshot, candidates = c
        collectors = list(s.collectors)
        changes = {}
        if phase == 0:
            changes['epoch'] = s.epoch + 1
            collectors[i] = (1, s.epoch + 1, None, ())
            label = f'collector{i}: advance to epoch={s.epoch + 1}'
        elif phase == 1:
            collectors[i] = (2, epoch, s.pointer, ())
            label = f'collector{i}: snapshot pointer={s.pointer}'
        elif phase == 2:
            candidates = tuple(sorted(k for k in s.objects if k[0] < epoch and k != snapshot))
            collectors[i] = (3, epoch, snapshot, candidates)
            label = f'collector{i}: list candidates={candidates}'
        elif phase == 3:
            if candidates:
                key, *rest = candidates
                changes['objects'] = s.objects - {key}
                collectors[i] = (3, epoch, snapshot, tuple(rest))
                label = f'collector{i}: delete {key}'
            else:
                collectors[i] = (4, None, None, ())
                label = f'collector{i}: finish'
        else:
            continue
        yield label, replace(s, collectors=tuple(collectors), **changes)
    phase, pointer = s.reader
    if phase == 0:
        yield f'reader: snapshot {s.pointer}', replace(s, reader=(1, s.pointer))
    elif phase in (1, 3):
        if pointer in s.objects:
            yield f'reader: return owned {pointer}', replace(s, reader=(4, None))
        else:
            # A provider error/retired fetch ends the operation unless a complete
            # fresh snapshot can replace the old pointer. One retry is finite.
            next_phase = 2 if phase == 1 else 4
            yield f'reader: missing {pointer}', replace(s, reader=(next_phase, pointer if next_phase != 4 else None))
    elif phase == 2:
        changed = s.pointer != pointer
        yield f'reader: refresh changed={changed}', replace(s, reader=(3, s.pointer) if changed else (4, None))


def explore(writers, collectors, fenced, epoch_keys=True):
    start = State(0, (0, 'A'), frozenset({(0, 'A')}),
                  ((0, None, None, None),) * writers,
                  ((0, None, None, ()),) * collectors)
    seen = set()
    path = []
    terminals = 0

    def visit(s):
        nonlocal terminals
        if s.pointer not in s.objects:
            return path.copy()
        if s in seen:
            return None
        if len(seen) >= MAX_STATES:
            raise RuntimeError(f'incomplete model: exceeded finite {MAX_STATES} states')
        seen.add(s)
        progress = False
        for label, after in successors(s, fenced, epoch_keys):
            progress = True
            path.append(label)
            failure = visit(after)
            if failure is not None:
                return failure
            path.pop()
        if not progress:
            terminals += 1
        return None

    failure = visit(start)
    return len(seen), terminals, failure


def main():
    for label, fenced, epoch_keys in [('no epoch CAS', False, True), ('no epoch namespace', True, False)]:
        count, _, failure = explore(2, 2, fenced, epoch_keys)
        assert failure is not None, f'the unsafe {label} control must fail'
        print(f'Negative control {label}: live-object loss reproduced after {count} states')
        for step in failure:
            print('  ' + step)
    for writers, collectors in [(2, 1), (1, 2), (2, 2)]:
        count, terminals, failure = explore(writers, collectors, True)
        assert failure is None, failure
        assert terminals > 0
        print(f'Fenced model writers={writers} collectors={collectors}: '
              f'{count} complete unique states, {terminals} terminal states; live pointers retain objects')
    print('Finite safety model only; no production implementation, provider-fault or physical-durability claim.')


if __name__ == '__main__':
    main()
