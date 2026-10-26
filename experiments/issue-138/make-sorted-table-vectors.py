#!/usr/bin/env python3
"""Independent sealed table fixtures; imports only the independent page oracle."""
import importlib.util
import json
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location('page_oracle', Path(__file__).with_name('make-native-page-vectors.py'))
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)
marker = '["native-sst1"]'
encode = lambda row: json.dumps(row, ensure_ascii=False, separators=(',', ':'))
rows = [['a', '1', ['\ufeff雪\0😀\n', '']], ['b', str((1 << 64)-1), None], ['c', '3', []]]
cases = []


def fixture(name, records, valid, expected=None, following=None, max_cells=252):
    image = dict(generation=0, root=0, rows=0, last_hash=0, pages=[], records=[],
                 metadata=[oracle.metadata(0, 0, 0, 0, 0, 0), bytes()])
    oracle.append_group(image, records, max_cells=max_cells)
    if following:
        oracle.append_group(image, following)
    cases.append(dict(name=name, valid=valid, rows=expected, **oracle.snapshot(image)))


fixture('empty', [marker], True, [])
fixture('values', [marker, *map(encode, rows)], True, rows)
fixture('multipage', [marker, encode(['a', '9', ['雪'*1500]]), encode(['z', '10', ['tail']])], True,
        [['a', '9', ['雪'*1500]], ['z', '10', ['tail']]])
fixture('escaped-json', [marker, json.dumps(rows[0], ensure_ascii=True)], True, [rows[0]])
fixture('unknown-marker', ['["native-sst2"]'], False)
fixture('missing-marker', [encode(rows[0])], False)
fixture('duplicate', [marker, encode(rows[0]), encode(rows[0])], False)
fixture('unsorted', [marker, encode(rows[1]), encode(rows[0])], False)
for name, value in [('zero', '0'), ('leading-zero','01'), ('negative','-1'), ('overflow',str(1 << 64)), ('numeric',1), ('newline','1\n')]:
    fixture('generation-'+name, [marker, encode(['a',value,[]])], False)
for name, row in [('empty-key',['','1',[]]), ('path-key',['a/b','1',[]]), ('long-key',['a'*129,'1',[]]),
                  ('numeric-record',['a','1',[1]]), ('object-record',['a','1',{}]), ('extra-field',['a','1',[],0])]:
    fixture(name, [marker, encode(row)], False)
fixture('second-flush', [marker, encode(rows[0])], False, following=[encode(rows[1])])
fixture('sparse-physical-pages', [marker, *map(encode, rows)], False, max_cells=5)
fixture('unpaired-surrogate', [marker, '["a","1",["\\ud800"]]'], False)
fixture('malformed-json', [marker, '{'], False)
fixture('nonstring-key', [marker, '[1,"1",[]]'], False)
result = dict(format='native-sst1', databaseId=oracle.IDENTITY.hex(), cases=cases)
Path(sys.argv[1] if len(sys.argv)>1 else 'docs/protocol/native-sorted-table-v1.json').write_text(json.dumps(result, ensure_ascii=True, indent=2)+'\n')
print(f'{len(cases)} independent sorted table vectors; {sum(case["valid"] for case in cases)} valid')
