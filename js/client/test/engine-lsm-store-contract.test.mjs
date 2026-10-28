import assert from 'node:assert/strict';
import test from 'node:test';
import { PhysicalLsmJournalStore as Store } from '../src/engine-lsm-store.js';
import { LsmHarness, selected } from './support/lsm-store.mjs';
const open=(h,options=selected)=>Store.open(h.wal,h.provider,options);
const put=(s,k,v)=>assert.equal(s.compareExchange([k],s.read([k]),[v]),true);

test('construction validates complete synchronous contracts and finite namespace admission',()=>{
  assert.throws(()=>new Store(),/use .*open/);
  for(const maxNamespaceEntries of [0,1,65537,1.5,NaN]) {
    const h=new LsmHarness();h.reset();assert.throws(()=>open(h,{...selected,maxNamespaceEntries}));assert.deepEqual(h.trace,[]);
  }
  for(const missing of ['wal','provider','replacement','read','create']) {
    const h=new LsmHarness();h.reset();
    if(missing==='replacement')Object.defineProperty(h.wal,'supportsReplacement',{value:false});
    if(missing==='read')h.wal.read=undefined;
    if(missing==='create')h.provider.create=undefined;
    assert.throws(()=>Store.open(missing==='wal'?undefined:h.wal,missing==='provider'?undefined:h.provider,selected));
    assert.deepEqual(h.trace,[]);
  }
  const h=new LsmHarness();const s=Store.open(h.wal,h.provider);s.flush();s.compact();s[Symbol.dispose]();s.close();
});
test('complete ordered namespace and record-array discovery reject before mutation',()=>{
  for(const names of [undefined,Array.from({length:17},(_,i)=>String(i+1)),['2','1'],['1','1'],['0'],['18446744073709551616']]) {
    const h=new LsmHarness();h.provider.list=()=>names;h.reset();assert.throws(()=>open(h));assert.deepEqual(h.trace,[]);
  }
  for(const text of [undefined,3,'{}','null','[3]']) {
    const h=new LsmHarness();h.wal.read=()=>text;h.reset();assert.throws(()=>open(h));assert.deepEqual(h.trace,[]);
  }
  const h=new LsmHarness();h.provider.list=()=>['1'];h.reset();assert.throws(()=>open(h),/uninitialized/);assert.deepEqual(h.trace,[]);
});
test('all malformed group inputs reject without journal or namespace effects',()=>{
  const h=new LsmHarness(),s=open(h);h.reset();const before=h.image();
  for(const keys of [null,[],Array(33).fill('a'),['a','a'],[undefined],[''],['a'.repeat(129)],['雪']])assert.throws(()=>s.read(keys));
  for(const value of [undefined,null,0,'','00','-1','x','18446744073709551616','1'.repeat(21)]) {
    assert.throws(()=>s.compareExchange(['a'],[{generation:value}],[[]]));
  }
  for(const [expected,updates] of [[null,[[]]],[[{generation:'0'}],null],[[],[[]]],[[{generation:'0'}],[]]])assert.throws(()=>s.compareExchange(['a'],expected,updates));
  for(const entries of [undefined,{},[null],[3],['\ud800']])assert.throws(()=>s.compareExchange(['a'],[{generation:'0'}],[entries]));
  assert.equal(s.compareDelete('missing','1'),false);assert.throws(()=>s.compareDelete('missing','0'));
  assert.deepEqual(h.trace,[]);assert.deepEqual(h.image(),before);s.close();
});
test('namespace scans apply key and byte bounds to live rows and own their results',()=>{
  const h=new LsmHarness(),s=open(h);put(s,'a',['A']);put(s,'b',[]);s.flush();
  assert.deepEqual(s.scanNamespace({keysOnly:true}).map(row=>row.version.entries),[[],[]]);
  assert.throws(()=>s.scanNamespace({maxKeys:1}),/scan admission/);
  assert.throws(()=>s.scanNamespace({maxBytes:9}),/scan admission/);
  const rows=s.scanNamespace();rows[0].version.entries.push('caller');assert.deepEqual(s.read(['a'])[0].entries,['A']);
  s.flush();s.compact();s.close();
});
test('point reads and close reject synchronous reentry while no mutation is pending',()=>{
  const h=new LsmHarness(),s=open(h);put(s,'a',['A']);s.flush();
  const device=[...h.tables.values()][0],read=device.readPage.bind(device);let checked=false;
  device.readPage=index=>{
    if(!checked){checked=true;assert.throws(()=>s.read(['a']),/reentry/);assert.throws(()=>s.close(),/reentry/);}
    return read(index);
  };
  assert.deepEqual(s.read(['a'])[0].entries,['A']);assert.equal(checked,true);s.close();
});
test('non-void or asynchronous mutation replies fence their owner without blind retries',()=>{
  for(const [method,result] of [['append',1],['append',{then(){}}],['flush',null],['replace',undefined]]) {
    const h=new LsmHarness();
    if(method==='replace') {h.wal.replace=()=>result;assert.throws(()=>open(h),/replacement result/);continue;}
    const s=open(h),original=h.wal[method].bind(h.wal);h.wal[method]=(...args)=>{original(...args);return result;};
    assert.throws(()=>put(s,'a',['A']),/synchronously/);assert.throws(()=>s.read(['a']),/fenced/);s.close();
  }
});
test('volatile devices remain explicitly volatile and invalid recovery methods reject acquired leases',()=>{
  const h=new LsmHarness();h.provider.supportsDurableFlush=false;Object.defineProperty(h.wal,'supportsDurableFlush',{value:false});
  const create=h.provider.create;h.provider.create=id=>{const device=create(id);device.supportsDurableFlush=false;return device;};
  const s=open(h);assert.equal(s.supportsDurableFlush,false);put(s,'a',['A']);s.flush();const image=h.image();s.close();
  const broken=new LsmHarness(image);[...broken.tables.values()][0].completeRecovery=3;
  assert.throws(()=>open(broken),/recovery must be synchronous/);assert.equal(broken.leases.size,0);
  const pending=new LsmHarness(image);[...pending.tables.values()][0].completeRecovery=()=>({then(){}});
  assert.throws(()=>open(pending),/synchronously/);assert.equal(pending.leases.size,0);
});
test('WAL replacement rotates bounded hot-key histories without forcing a table flush',()=>{
  for(const bounds of [{maxWalRecords:2},{maxWalRecords:100,maxWalBytes:180}]) {
    const h=new LsmHarness(),s=open(h,{...selected,...bounds,bufferKeys:32});
    for(let i=0;i<12;i++)put(s,'a',[String(i)]);
    assert.equal(h.tables.size,0);assert.ok(JSON.parse(h.wal.read()).length<12);s.close();
    const reopened=open(new LsmHarness(h.image()),{...selected,...bounds,bufferKeys:32});assert.deepEqual(reopened.read(['a'])[0].entries,['11']);reopened.close();
  }
});
test('buffer, total capacity, WAL record and complete namespace bounds precede effects',()=>{
  for(const [options,action] of [
    [{...selected,maxWalRecords:1},s=>put(s,'a',[])],
    [{...selected,maxKeys:1,bufferKeys:1},s=>{put(s,'a',[]);return ()=>put(s,'b',[]);} ],
    [{...selected,maxBytes:16,bufferBytes:16},s=>put(s,'a',['x'.repeat(8)])],
  ]) {
    const h=new LsmHarness(),s=open(h,options);let pending=()=>action(s);
    if(options.maxKeys===1)pending=action(s);
    h.reset();const before=h.image();assert.throws(pending);assert.deepEqual(h.trace,[]);assert.deepEqual(h.image(),before);s.close();
  }
  const h=new LsmHarness(),s=open(h,{...selected,maxNamespaceEntries:2,maxRuns:4});
  for(const key of ['a','b']){put(s,key,[]);s.flush();}put(s,'c',[]);h.reset();const before=h.image();
  assert.throws(()=>s.flush(),/namespace admission/);assert.deepEqual(h.trace,[]);assert.deepEqual(h.image(),before);s.close();
});
