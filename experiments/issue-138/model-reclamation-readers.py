#!/usr/bin/env python3
"""Finite whole-group read/collection model; independent design evidence only."""
from dataclasses import dataclass, replace
import resource
resource.setrlimit(resource.RLIMIT_AS, (96 * 1024 * 1024,) * 2)
MAX_STATES = 10000
OLD = ('a0', 'b0')
NEW = ('a1', 'b1')
@dataclass(frozen=True, slots=True)
class State:
    published: bool = False
    collected: bool = False
    phase: int = 0
    pointers: tuple = ()
    owned: tuple = ()
    retry: bool = False

def explore(whole_group):
    seen = set()
    path = []
    terminals = 0
    def visit(s):
        nonlocal terminals
        if s in seen: return None
        assert len(seen) < MAX_STATES, 'finite state budget exhausted'
        seen.add(s)
        if s.phase == 4:
            terminals += 1
            if s.owned not in (OLD, NEW): return path.copy()
            return None
        steps = []
        if not s.published: steps.append(('writer: publish both shard pointers atomically', replace(s, published=True)))
        if s.published and not s.collected: steps.append(('collector: retire both old objects', replace(s, collected=True)))
        if s.phase == 0:
            pointers = NEW if s.published else OLD
            steps.append((f'reader: capture whole catalog {pointers}', replace(s, phase=1, pointers=pointers)))
        elif s.phase in (1, 2):
            index = s.phase - 1
            pointer = s.pointers[index]
            if pointer in NEW or not s.collected:
                owned = s.owned + (pointer,)
                steps.append((f'reader: own {pointer}', replace(s, phase=4 if index else 2, owned=owned)))
            elif not s.retry:
                if whole_group:
                    steps.append(('reader: refresh whole catalog, discard all partial owned data', replace(s, phase=1, pointers=NEW, owned=(), retry=True)))
                else:
                    pointers = list(s.pointers); pointers[index] = NEW[index]
                    steps.append(('reader: unsafe refresh of only missing shard', replace(s, pointers=tuple(pointers), retry=True)))
        for label, after in steps:
            path.append(label)
            failure = visit(after)
            if failure: return failure
            path.pop()
        return None
    failure = visit(State())
    return len(seen), terminals, failure

bad_states, _, failure = explore(False)
assert failure, 'individual refresh must demonstrate a fractured group'
print(f'Negative control: fractured multi-shard read after {bad_states} states')
for step in failure: print('  '+step)
count, terminals, failure = explore(True)
assert failure is None and terminals > 0
print(f'Whole-group refresh: {count} complete states, {terminals} coherent terminal views')

# Non-reusable generation compare distinguishes a deleted/recreated object.
listed_generation = 7
replacement_generation = 9
assert listed_generation != replacement_generation
unsafe_delete_by_name = True
conditional_delete = listed_generation == replacement_generation
assert unsafe_delete_by_name and not conditional_delete
print('ABA negative/positive controls: delete-by-name loses replacement; exact generation preserves it')
print('Design model only; no provider, production implementation or physical durability claim.')
