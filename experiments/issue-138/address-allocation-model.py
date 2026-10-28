#!/usr/bin/env python3
"""Independent finite reservation oracle; imports no link-cli implementation."""
import itertools
import json
from pathlib import Path
import sys

MAX = (1 << 64) - 1
A, B = f'{1:032x}', f'{2:032x}'

def wire(value):
    return json.dumps(value, separators=(',', ':'))


def allocate(history, author, token, count, first=1, last=5):
    for old in history:
        if old[1] == token:
            if old[0] != author or old[2] != count:
                return history, None
            return history, old
    start = history[-1][4] + 1 if history else first
    if count <= 0 or start + count - 1 > last:
        return history, None
    grant = (author, token, count, start, start + count - 1)
    return (*history, grant), grant


def check(history):
    occupied = []
    for grant in history:
        values = list(range(grant[3], grant[4] + 1))
        assert len(values) == grant[2]
        assert not set(occupied).intersection(values)
        occupied.extend(values)
    assert occupied == list(range(1, 1 + len(occupied)))


def fixtures():
    header = wire(['address-reservations1', '1', '5'])
    grant = wire(['reserve-addresses1', A, A, '2', '1', '2'])
    cases = []
    def add(name, entries, valid=False, generation='1', first='1', last='5', next_value=None, reservations=None, options=None):
        item = dict(name=name, first=first, last=last, journal=dict(generation=generation, entries=entries), valid=valid, options=options or {})
        if valid:
            item['next'] = next_value
            item['reservations'] = reservations or []
        cases.append(item)
    add('absent', [], True, generation='0', next_value='1')
    add('initialized-empty', [header], True, next_value='1')
    add('first-grant', [header, grant], True, next_value='3', reservations=[dict(author=A, token=A, count='2', first='1', last='2')])
    maximum_header = wire(['address-reservations1', str(MAX-1), str(MAX)])
    maximum_grant = wire(['reserve-addresses1', A, A, '2', str(MAX-1), str(MAX)])
    add('maximum-exhaustion', [maximum_header, maximum_grant], True, first=str(MAX-1), last=str(MAX), reservations=[dict(author=A, token=A, count='2', first=str(MAX-1), last=str(MAX))])
    add('full-domain', [wire(['address-reservations1', '1', str(MAX)]), wire(['reserve-addresses1', A, A, str(MAX), '1', str(MAX)])], True, last=str(MAX), reservations=[dict(author=A, token=A, count=str(MAX), first='1', last=str(MAX))])
    add('zero-generation-with-bytes', [header], generation='0')
    add('missing-header', [])
    add('wrong-header', ['[]'])
    add('wrong-domain', [wire(['address-reservations1','2','5'])])
    add('header-whitespace', [' ' + header])
    add('header-escape', [header.replace('address', r'\u0061ddress')])
    add('duplicate-token', [header, grant, wire(['reserve-addresses1', B, A, '1','3','3'])])
    add('gap', [header, wire(['reserve-addresses1', A, A, '1','2','2'])])
    add('overlap', [header, grant, wire(['reserve-addresses1', B, B, '2','2','3'])])
    add('past-domain', [header, wire(['reserve-addresses1', A, A, '6','1','6'])])
    add('wrong-width', [header, wire(['reserve-addresses1', A, A, '2','1','3'])])
    add('record-whitespace', [header, ' '+grant])
    add('record-object', [header, '{}'])
    add('record-null', [header, 'null'])
    add('record-json-error', [header, '['])
    add('record-too-long', [header, ' '*257])
    add('record-depth', [header, '['*100+']'*100])
    add('record-budget', [header, grant], options=dict(maxEntries=2, maxBytes=len(header)+len(grant)-1))
    add('entry-budget', [header, grant, grant], options=dict(maxEntries=2))
    base=['reserve-addresses1', A, A, '2','1','2']
    for index, values in [(0,['wrong', None]), (1,['0'*32, 'A'*32, '', 'bad']), (2,['0'*32, 'F'*32, None, 'bad']), (3,['0','01','-1',str(MAX+1), 2, None]), (4,['0','01',str(MAX+1), 1]), (5,['0','02','-1', str(MAX+1), 2])]:
        for n,value in enumerate(values):
            changed=base.copy(); changed[index]=value
            add(f'bad-field-{index}-{n}', [header, wire(changed)])
    for length in [0,1,5,7]:
        add(f'wrong-fields-{length}', [header, wire((base+['extra'])[:length])])
    return dict(format='address-reservations1', cases=cases)


def main():
    requests=list(itertools.product([A,B], [A,B,'3'*32], [1,2,3]))
    count=0
    for schedule in itertools.product(requests, repeat=3):
        history=()
        for request in schedule:
            before=history; history,grant=allocate(history,*request); check(history)
            if grant is None: assert history==before
            else:
                # Exact retry after arbitrary following allocations is observational.
                retried,again=allocate(history,grant[0],grant[1],grant[2])
                assert retried==history and again==grant
        count+=1
    assert count==5832
    # Two clients load the same state: only one can publish its candidate.
    orderings=0
    for order in itertools.permutations(['readA','readB','casA','casB']):
        if order.index('readA')>order.index('casA') or order.index('readB')>order.index('casB'): continue
        state=(); seen={}; revision=0
        for event in order:
            who=event[-1]
            if event.startswith('read'): seen[who]=(revision,state)
            elif seen[who][0]==revision:
                author=A if who=='A' else B
                state,_=allocate(seen[who][1],author,author,2); revision+=1
        check(state)
        for author in [A,B]: state,_=allocate(state,author,author,2)
        assert len(state)==2 and state[0][4]<state[1][3]
        orderings+=1
    assert orderings==6
    if len(sys.argv)>1: Path(sys.argv[1]).write_text(json.dumps(fixtures(),indent=2)+'\n')
    print(f'PASS {count} independent finite schedules, {orderings} complete two-writer interleavings; {len(fixtures()["cases"])} canonical/admission fixtures')

if __name__=='__main__': main()
