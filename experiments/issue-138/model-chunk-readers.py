#!/usr/bin/env python3
"""Finite chunk-reader retirement model, independent of production/providers."""
from dataclasses import dataclass, replace
import resource

resource.setrlimit(resource.RLIMIT_AS, (128 * 1024 * 1024,) * 2)
MAX_STATES = 100000
VIEWS = tuple(tuple(f'{shard}{revision}.{chunk}' for shard in 'ab' for chunk in range(2)) for revision in range(3))

@dataclass(frozen=True, slots=True)
class State:
    published: int = 0
    collected: int = 0
    selected: tuple = ()
    owned: tuple = ()
    retry: bool = False
    phase: str = 'capture'


def explore(whole_group, cache_mask):
    seen, path = set(), []
    terminals = failures = 0
    def visit(s):
        nonlocal terminals, failures
        if s in seen:
            return None
        assert len(seen) < MAX_STATES, 'finite state budget exhausted'
        seen.add(s)
        if s.phase == 'done':
            terminals += 1
            return path.copy() if s.owned not in VIEWS else None
        if s.phase == 'fail':
            failures += 1
            return None
        steps = []
        if s.published < 2:
            steps.append((f'writer: publish complete view {s.published + 1}', replace(s, published=s.published + 1)))
        if s.collected < s.published:
            steps.append((f'collector: retire every view below {s.published}', replace(s, collected=s.published)))
        if s.phase == 'capture':
            steps.append((f'reader: capture four references from view {s.published}', replace(s, selected=VIEWS[s.published], phase='read')))
        elif s.phase == 'read':
            index = len(s.owned)
            item = s.selected[index]
            revision = next(i for i, view in enumerate(VIEWS) if item in view)
            cached = revision == 0 and (cache_mask & (1 << index)) != 0
            if revision >= s.collected or cached:
                owned = s.owned + (item,)
                steps.append((f'reader: own {item} ({"cache" if cached else "provider"})', replace(s, owned=owned, phase='done' if len(owned) == 4 else 'read')))
            elif s.retry or s.selected == VIEWS[s.published]:
                steps.append(('reader: fail closed after bounded retry or unchanged root', replace(s, phase='fail')))
            elif whole_group:
                steps.append(('reader: discard ALL prefixes/shards/cache-hit values and refresh whole view', replace(s, selected=VIEWS[s.published], owned=(), retry=True)))
            else:
                selected = list(s.selected)
                selected[index] = VIEWS[s.published][index]
                steps.append(('reader: unsafe refresh of only missing chunk; keep owned prefix', replace(s, selected=tuple(selected), retry=True)))
        for label, after in steps:
            path.append(label)
            failure = visit(after)
            if failure:
                return failure
            path.pop()
        return None
    failure = visit(State())
    return len(seen), terminals, failures, failure

states, _, _, failure = explore(False, 1)
assert failure, 'unsafe per-chunk refresh must fracture retained prefixes'
print(f'NEGATIVE per-chunk refresh: fractured view after {states} states')
for step in failure:
    print('  ' + step)
total_states = total_views = total_failures = 0
for mask in range(16):
    states, terminals, failures, failure = explore(True, mask)
    assert failure is None and terminals > 0
    total_states += states
    total_views += terminals
    total_failures += failures
print(f'PASS whole-group chunk refresh: {total_states} complete states across all 16 cache masks; {total_views} coherent terminal views; {total_failures} bounded fail-closed terminals')
print('Two atomic publishers, two possible retirement events, two shards / four chunks, one retry.')
print('Finite model only; no production/provider/fault/physical-durability claim.')
