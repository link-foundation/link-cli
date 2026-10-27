#!/usr/bin/env python3
"""Independent finite native-lsm1 semantic oracle; imports no production code."""
import copy
import json
import re
import sys
from pathlib import Path

MAX = 2**64 - 1
DEFAULT = dict(maxKeys=4096, maxBytes=8388608, bufferKeys=64,
               bufferBytes=1048576, maxRuns=4, maxWalRecords=1024,
               maxWalBytes=16777216)


def canonical(value, positive=False):
    assert isinstance(value, str) and len(value) <= 20
    assert re.fullmatch(r'0|[1-9][0-9]*', value) and int(value) <= MAX
    assert not positive or value != '0'
    return int(value)


def rows(value, lower, upper, limit, byte_limit):
    assert isinstance(value, list) and len(value) <= limit
    previous = ''
    size = 0
    for row in value:
        assert isinstance(row, list) and len(row) == 3
        key, generation, entries = row
        assert isinstance(key, str) and re.fullmatch(r'[A-Za-z0-9_.-]{1,128}', key)
        assert key > previous and lower < canonical(generation, True) <= upper
        previous = key
        size += len(key) + 8
        assert entries is None or isinstance(entries, list)
        if entries is not None:
            for entry in entries:
                assert isinstance(entry, str)
                size += len(entry.encode('utf-8')) + 1
        assert size <= byte_limit
    return copy.deepcopy(value)


def recover(records, selected=None):
    options = DEFAULT | (selected or {})
    limits = dict(maxKeys=65536, maxBytes=8388608, bufferKeys=65536,
                  bufferBytes=8388608, maxRuns=64, maxWalRecords=65536,
                  maxWalBytes=67108864)
    for key, maximum in limits.items():
        assert type(options[key]) is int and 1 <= options[key] <= maximum
    assert options['bufferKeys'] <= options['maxKeys']
    assert options['bufferBytes'] <= options['maxBytes']
    assert isinstance(records, list) and 1 <= len(records) <= options['maxWalRecords']
    size = 0
    parsed = []
    for record in records:
        assert isinstance(record, str) and '\r' not in record and '\n' not in record
        size += len(record.encode('utf-8'))
        assert size <= options['maxWalBytes']
        parsed.append(json.loads(record))
    checkpoint = parsed[0]
    assert isinstance(checkpoint, list) and len(checkpoint) == 6 and checkpoint[0] == 'native-lsm1'
    _, global_text, covered_text, reserved_text, manifest, buffer = checkpoint
    generation, covered, reserved = map(canonical, [global_text, covered_text, reserved_text])
    assert covered <= generation
    assert isinstance(manifest, list) and len(manifest) <= options['maxRuns']
    previous = 0
    for item in manifest:
        assert isinstance(item, list) and len(item) == 5
        identity, native_id, digest, count, stored = item
        identity = canonical(identity, True)
        assert previous < identity <= reserved
        previous = identity
        assert isinstance(native_id, str) and re.fullmatch(r'[0-9a-f]{32}', native_id)
        canonical(digest)
        assert canonical(count) <= options['maxKeys']
        assert 15 <= canonical(stored) <= 67108864
    assert (covered != 0) == bool(manifest)
    buffer = rows(buffer, covered, generation, options['bufferKeys'], options['bufferBytes'])
    assert (max((int(row[1]) for row in buffer), default=covered)) == generation
    for record in parsed[1:]:
        assert isinstance(record, list) and len(record) == 3 and record[0] == 'put'
        following = canonical(record[1], True)
        assert following == generation + 1
        changes = rows(record[2], generation, following, 32, options['bufferBytes'])
        assert changes
        merged = {row[0]: row for row in buffer}
        merged.update({row[0]: row for row in changes})
        buffer = rows(sorted(merged.values()), covered, following,
                      options['bufferKeys'], options['bufferBytes'])
        generation = following
    return ['native-lsm1', str(generation), str(covered), str(reserved), manifest, buffer]


def main():
    cases = []
    empty = ['native-lsm1', '0', '0', '0', [], []]
    table = ['1', '000102030405060708090a0b0c0d0e0f', str(MAX), '3', '200']

    def add(name, values, valid, options=None, raw=False):
        records = values if raw else [json.dumps(v, ensure_ascii=True, separators=(',', ':')) for v in values]
        case = dict(name=name, records=records, valid=valid)
        if options is not None:
            case['options'] = options
        try:
            state = recover(records, options)
        except (AssertionError, ValueError, TypeError, UnicodeError):
            assert not valid, name
        else:
            assert valid, name
            case['checkpoint'] = state
        cases.append(case)

    add('empty', [empty], True)
    add('reserved-orphan-max-id', [empty[:3] + [str(MAX), [], []]], True)
    add('unicode-tombstone-empty-and-overwrite', [empty,
        ['put', '1', [['a', '1', ['\ufeff雪\0😀\n']], ['b', '1', None], ['c', '1', []]]],
        ['put', '2', [['a', '2', ['new']], ['d', '2', []]]]], True)
    add('selected-table-and-buffer', [['native-lsm1', '4', '2', '3', [table], [['a', '4', None]]]], True)
    add('covered-generation-max', [['native-lsm1', str(MAX), str(MAX), str(MAX), [table], []]], True)
    add('last-generation-usable', [['native-lsm1', str(MAX-1), str(MAX-1), '1', [table], []], ['put', str(MAX), [['z', str(MAX), []]]]], True)
    add('two-ordered-runs', [['native-lsm1', '2', '2', '2', [table, ['2', 'f'*32, '0', '0', '15']], []]], True)
    add('replay-inserts-before-buffer-key', [empty, ['put','1',[['z','1',[]]]], ['put','2',[['a','2',[]]]]], True)
    add('escaped-key-and-json-whitespace', [' ["native-lsm1","1","0","0",[],[["\\u0061","1",[]]]] '], True, raw=True)
    for name, value in [('unknown-marker', ['native-lsm2', '0','0','0',[],[]]), ('missing-field', empty[:-1]), ('extra-field', empty+[0]), ('object-checkpoint', {})]:
        add(name, [value], False)
    for position in [1, 2, 3]:
        for value in [0, None, '', '01', '-1', '1\n', str(MAX+1), '1'*21]:
            item = copy.deepcopy(empty); item[position] = value
            add(f'counter-{position}-{repr(value)}', [item], False)
    add('covered-after-global', [['native-lsm1','1','2','1',[table],[]]], False)
    add('missing-selected-table', [['native-lsm1','1','1','0',[],[]]], False)
    add('table-with-zero-covered', [['native-lsm1','0','0','1',[table],[]]], False)
    for name, index, value in [('zero-id',0,'0'),('unreserved-id',0,'2'),('upper-identity',1,'A'*32),('short-identity',1,'0'*31),('bad-digest',2,'01'),('count-type',3,3),('count-negative',3,'-1'),('count-overflow',3,'65537'),('bytes-small',4,'14'),('bytes-type',4,15),('bytes-overflow',4,'67108865')]:
        item = copy.deepcopy(table); item[index] = value
        add(name, [['native-lsm1','1','1','1',[item],[]]], False)
    add('duplicate-table', [['native-lsm1','1','1','1',[table,table],[]]], False)
    add('reverse-table-order', [['native-lsm1','1','1','2',[['2','f'*32,'0','0','15'],table],[]]], False)
    add('too-many-runs', [['native-lsm1','1','1','1',[table,table],[]]], False, {'maxRuns':1})
    add('short-descriptor', [['native-lsm1','1','1','1',[table[:-1]],[]]], False)
    add('non-array-descriptor', [['native-lsm1','1','1','1',[{}],[]]], False)
    add('non-array-manifest', [['native-lsm1','0','0','0',{},[]]], False)
    add('non-array-buffer', [['native-lsm1','0','0','0',[],{}]], False)
    for name, value in [('empty-key',''),('path-key','a/b'),('long-key','a'*129),('newline-key','a\n'),('numeric-key',1)]:
        add(name, [['native-lsm1','1','0','0',[],[[value,'1',[]]]]], False)
    for name, row in [('zero-row',['a','0',[]]),('future-row',['a','2',[]]),('row-shape',['a','1',[],0]),('numeric-entry',['a','1',[1]]),('object-payload',['a','1',{}]),('surrogate',['a','1',['\ud800']])]:
        add(name, [['native-lsm1','1','0','0',[],[row]]], False)
    add('duplicate-row', [['native-lsm1','1','0','0',[],[['a','1',[]],['a','1',[]]]]], False)
    add('reverse-row', [['native-lsm1','1','0','0',[],[['b','1',[]],['a','1',[]]]]], False)
    add('covered-buffer-row', [['native-lsm1','2','1','1',[table],[['a','1',[]],['b','2',[]]]]], False)
    add('missing-latest-row', [['native-lsm1','2','0','0',[],[['a','1',[]]]]], False)
    add('empty-buffer-new-generation', [['native-lsm1','1','0','0',[],[]]], False)
    for name, record in [('unknown-group',['push','1',[['a','1',[]]]]),('empty-group',['put','1',[]]),('skipped-generation',['put','2',[['a','2',[]]]]),('mixed-generation',['put','1',[['a','2',[]]]]),('repeated-checkpoint',empty),('extra-group-field',['put','1',[['a','1',[]]],0]),('too-many-changes',['put','1',[[f'k{i:02}','1',[]] for i in range(33)]])]:
        add(name, [empty,record], False)
    add('repeated-generation', [empty,['put','1',[['a','1',[]]]],['put','1',[['b','1',[]]]]], False)
    add('generation-overflow', [['native-lsm1',str(MAX),str(MAX),'1',[table],[]],['put',str(MAX+1),[['a',str(MAX+1),[]]]]], False)
    add('buffer-key-limit', [empty,['put','1',[['a','1',[]],['b','1',[]]]]], False, {'bufferKeys':1})
    add('buffer-byte-limit', [empty,['put','1',[['a','1',['x']]]]], False, {'bufferBytes':10})
    add('empty-row-still-charges-key-generation', [empty,['put','1',[['a','1',[]]]]], False, {'bufferBytes':8})
    add('tombstone-still-charges-key-generation', [empty,['put','1',[['a','1',None]]]], False, {'bufferBytes':8})
    add('overwrites-do-not-accumulate-buffer-cost', [empty,['put','1',[['a','1',['x']]]],['put','2',[['a','2',[]]]]], True, {'bufferKeys':1,'bufferBytes':11})
    add('wal-record-limit', [empty,['put','1',[['a','1',[]]]]], False, {'maxWalRecords':1})
    encoded = json.dumps(empty, separators=(',', ':'))
    add('wal-exact-bytes', [encoded], True, {'maxWalBytes':len(encoded)}, True)
    add('wal-byte-limit', [encoded], False, {'maxWalBytes':len(encoded)-1}, True)
    for name, records in [('empty-wal',[]),('bad-json',['[']),('raw-line-break',[encoded+'\n']),('non-string',[empty])]:
        add(name, records, False, raw=True)
    for option in DEFAULT:
        add('zero-'+option, [empty], False, {option:0})
    for option, maximum in dict(maxKeys=65536,maxBytes=8388608,bufferKeys=65536,
            bufferBytes=8388608,maxRuns=64,maxWalRecords=65536,maxWalBytes=67108864).items():
        add('excessive-'+option, [empty], False, {option:maximum+1})
    add('buffer-larger-than-total-keys', [empty], False, {'maxKeys':1})
    add('buffer-larger-than-total-bytes', [empty], False, {'maxBytes':1})
    destination = Path(sys.argv[1])
    destination.write_text(json.dumps(dict(format='native-lsm1', cases=cases), indent=2, ensure_ascii=True)+'\n')
    print(f'PASS independent native-lsm1 oracle: {len(cases)} cases ({sum(c["valid"] for c in cases)} valid)')


if __name__ == '__main__':
    main()
