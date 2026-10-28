#!/usr/bin/env python3
"""Independent finite takeover/guard-CAS schedules and canonical fixtures."""
import itertools
import json
import sys
from pathlib import Path

A = '0' * 31 + '1'
T = ['0' * 31 + str(i) for i in range(2, 5)]
wire = lambda x: json.dumps(x, separators=(',', ':'))
header = wire(['author-fence1', A])
def record(i): return wire(['claim-author1', T[i - 1], str(i)])

# Every complete interleaving preserves each process's read-before-CAS order.
count = 0
for events in itertools.permutations(['read-old', 'cas-old', 'read-new', 'cas-new']):
    if events.index('read-old') > events.index('cas-old') or events.index('read-new') > events.index('cas-new'):
        continue
    epoch = 1
    seen = {}
    data = []
    taken = False
    for event in events:
        op, owner = event.split('-')
        if op == 'read': seen[owner] = epoch if owner == 'new' or epoch == 1 else None
        elif owner == 'new':
            if seen[owner] == epoch:
                epoch += 1
                taken = True
        elif seen[owner] == epoch:
            assert not taken
            data.append('old')
    count += 1
assert count == 6
# Negative control: checking ownership only in an earlier read is insufficient.
# Old reads epoch 1; takeover publishes epoch 2; a data-only CAS still succeeds.
fence_generation, data_generation = 1, 0
old_fence, old_data = fence_generation, data_generation
fence_generation += 1
assert old_data == data_generation  # vulnerable publication would be admitted
assert old_fence != fence_generation  # grouped comparison refuses it

cases = []
def case(name, generation, entries, valid=False, epoch=0, claims=None):
    cases.append(dict(name=name, journal=dict(generation=str(generation), entries=entries), valid=valid,
                      epoch=str(epoch), claims=claims or []))
def claim(i): return dict(author=A, token=T[i-1], epoch=str(i))
case('absent', 0, [], True)
case('header', 1, [header], True)
case('one', 2, [header, record(1)], True, 1, [claim(1)])
case('following', 3, [header, record(1), record(2)], True, 2, [claim(1), claim(2)])
case('maximum-generation', 18446744073709551615, [header, record(1)], True, 1, [claim(1)])
for name, generation, entries in [
    ('absent-with-header', 0, [header]), ('present-empty', 1, []),
    ('wrong-author', 1, [wire(['author-fence1', T[0]])]),
    ('spaced-header', 1, [' ' + header]), ('missing-header', 1, [record(1)]),
    ('epoch-gap', 1, [header, record(2)]),
    ('unknown-claim-tag', 1, [header, wire(['unknown-author1', T[0], '1'])]),
    ('duplicate-token', 2, [header, record(1), wire(['claim-author1', T[0], '2'])]),
    ('noncanonical-epoch', 1, [header, wire(['claim-author1', T[0], '01'])]),
    ('zero-token', 1, [header, wire(['claim-author1', '0'*32, '1'])]),
    ('uppercase-token', 1, [header, wire(['claim-author1', 'F'*32, '1'])]),
    ('numeric-epoch', 1, [header, wire(['claim-author1', T[0], 1])]),
    ('extra-field', 1, [header, wire(['claim-author1', T[0], '1', 'extra'])]),
    ('nested', 1, [header, '['*65 + '0' + ']'*65]),
    ('oversized', 1, [header, 'x'*257]),
    ('oversized-utf8-before-parse', 1, [header, '雪'*100]),
    ('non-ascii-token', 1, [header, '["claim-author1","' + '0'*31 + '雪","1"]']),
    ('object', 1, [header, '{}']), ('null', 1, [header, 'null']),
    ('escaped-token', 1, [header, record(1).replace('000', '\\u003000', 1)]),
]: case(name, generation, entries)
Path(sys.argv[1]).write_text(json.dumps(dict(author=A, cases=cases), indent=2) + '\n')
print(f'PASS independent author fencing: {count} complete takeover/publication schedules; {len(cases)} wire fixtures')
