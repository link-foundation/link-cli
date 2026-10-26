#!/usr/bin/env python3
"""Independent canonical etcd gateway fixtures; no production imports."""
import base64
import copy
import json
import sys
from pathlib import Path

def b64(value):
    return base64.b64encode(value.encode()).decode()
def kv(key='a', generation='7', entries=None, prefix='/test/'):
    return dict(key=b64(prefix+key), mod_revision=generation, value=b64(json.dumps(entries or [], ensure_ascii=False, separators=(',', ':'))))
def page(values=(), revision='10', more=False, count=None):
    result = dict(header=dict(revision=revision), succeeded=True, responses=[dict(response_range=dict(kvs=list(values)))])
    if more: result['responses'][0]['response_range']['more'] = True
    if count is not None: result['responses'][0]['response_range']['count'] = str(count)
    elif values: result['responses'][0]['response_range']['count'] = str(len(values))
    return result
scans=[]
def add(name, replies, expected=None, options=None, prefix='/test/'):
    scans.append(dict(name=name, replies=replies, options=options or {}, prefix=prefix, expected=expected))
def entry(key, generation='7', entries=None):
    return dict(key=key, version=dict(generation=generation, entries=entries or []))
add('empty namespace', [page()], [])
add('explicit zero count', [page(count=0)], [])
add('pinned pagination', [page([kv('a', entries=['first 雪']), kv('b')], more=True, count=3), page([kv('z', generation='9')], revision='12')], [entry('a', entries=['first 雪']), entry('b'), entry('z','9')], dict(pageKeys=2))
add('maximum signed revision', [page([kv(generation='9223372036854775807')], revision='9223372036854775807')], [entry('a','9223372036854775807')])
add('unicode namespace', [page([kv(prefix='/雪/', entries=['héllo\n😀'])])], [entry('a',entries=['héllo\n😀'])], prefix='/雪/')
add('keys only', [page([dict(key=b64('/test/a'),mod_revision='7')])], [entry('a')], dict(keysOnly=True))
add('keys only explicit empty value', [page([dict(key=b64('/test/a'),mod_revision='7',value='')])], [entry('a')], dict(keysOnly=True))
for name,value in [('null snapshot',None),('array snapshot',[]),('empty snapshot',{}),('false snapshot',dict(succeeded=False))]: add(name,[value])
valid=page([kv()])
def mutate(name, path, value):
    item=copy.deepcopy(valid); target=item
    for part in path[:-1]: target=target[part]
    target[path[-1]]=value
    add(name,[item])
for value in [None,'', '0','01','+1','9223372036854775808',10]: mutate('bad revision '+repr(value), ['header','revision'],value)
for value in [None,False,'true']: mutate('bad success '+repr(value),['succeeded'],value)
for value in [None,[],[{},{}],[{}]]: mutate('bad responses '+repr(value),['responses'],value)
for value in [None,[],7]: mutate('bad range '+repr(value),['responses',0,'response_range'],value)
for value in [None,{},7]: mutate('bad kvs '+repr(value),['responses',0,'response_range','kvs'],value)
for value in [None,0,'false']: mutate('bad more '+repr(value),['responses',0,'response_range','more'],value)
for value in [None,'0','01','2','9223372036854775808',1]: mutate('bad count '+repr(value),['responses',0,'response_range','count'],value)
for value in [None,'0','01','11','9223372036854775808',7]: mutate('bad generation '+repr(value),['responses',0,'response_range','kvs',0,'mod_revision'],value)
for value in [None,'!','L3Rlc3QvYQ==\n',b64('/foreign/a'),b64('/test/'),b64('/test/a/b'),b64('/test/'+'a'*129),'/w==']:
    mutate('bad key '+repr(value),['responses',0,'response_range','kvs',0,'key'],value)
for value in [None,'!','W10=\n',b64('null'),b64('[null]'),b64('[7]'),b64('123'),b64('[\"\\ud800\"]'),b64('[\"\\udc00\"]'),b64('{'),'/w==']:
    mutate('bad value '+repr(value),['responses',0,'response_range','kvs',0,'value'],value)
add('missing nonempty count',[dict(header={'revision':'10'},succeeded=True,responses=[dict(response_range=dict(kvs=[kv()]))])])
add('empty more',[page(more=True,count=1)])
add('more contradicts count',[page([kv()],more=True,count=1)])
add('duplicate keys',[page([kv(),kv()])])
add('reversed keys',[page([kv('z'),kv('a')])])
add('keys only returned data',[page([kv()])],options=dict(keysOnly=True))
for name,second in [('later revision reversal',page([kv('b')],revision='9')),('later generation beyond pin',page([kv('b',generation='11')],revision='12')),('later duplicate',page([kv()]))]:
    add(name,[page([kv()],more=True,count=2),second],options=dict(pageKeys=1))
add('key limit',[page([kv(),kv('b')])],options=dict(maxKeys=1))
add('more beyond key limit',[page([kv()],more=True,count=2)],options=dict(maxKeys=1,pageKeys=1))
add('byte limit',[page([kv(entries=['雪'])])],options=dict(maxBytes=12))
add('exact byte limit',[page([kv(entries=['雪'])])],[entry('a',entries=['雪'])],dict(maxBytes=13))
add('page limit',[page([kv(),kv('b')])],options=dict(pageKeys=1))
deletes=[]
def delete(name,result,expected=None): deletes.append(dict(name=name,reply=result,expected=expected))
delete('omitted rejection',dict(header={'revision':'10'}),False)
delete('explicit rejection',dict(header={'revision':'10'},succeeded=False,responses=[]),False)
delete('acknowledged deletion',dict(header={'revision':'10'},succeeded=True,responses=[dict(response_delete_range={'deleted':'1'})]),True)
for name,result in [('null',None),('missing header',{}),('zero revision',dict(header={'revision':'0'})),('bad success',dict(header={'revision':'10'},succeeded=None)),('missing deletion',dict(header={'revision':'10'},succeeded=True)),('rejection with results',dict(header={'revision':'10'},responses=[{}])),('bad rejection results',dict(header={'revision':'10'},responses=None))]:delete(name,result)
for value in [None,'0','2','01',1]:delete('invalid deletion '+repr(value),dict(header={'revision':'10'},succeeded=True,responses=[dict(response_delete_range={'deleted':value})]))
path=Path(sys.argv[1]) if len(sys.argv)>1 else Path('docs/protocol/journal-maintenance-v1.json');path.write_text(json.dumps(dict(scans=scans,deletes=deletes),ensure_ascii=False,indent=2)+'\n')
print(f'{len(scans)} scan cases, {len(deletes)} delete cases')
