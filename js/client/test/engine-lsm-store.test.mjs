import assert from 'node:assert/strict';
import test from 'node:test';
import { PhysicalLsmJournalStore } from '../src/engine-lsm-store.js';
import { LsmWal } from '../src/engine-lsm-wal.js';
import { LsmHarness, selected } from './support/lsm-store.mjs';
const put = (store, key, value) => assert.equal(store.compareExchange([key], store.read([key]), [value]), true);
const open = h => PhysicalLsmJournalStore.open(h.wal,h.provider,selected);
function prepared() {
  const h = new LsmHarness(), store = open(h);
  put(store,'a',['A']); put(store,'b',[]); store.flush();
  put(store,'a',['new']); assert.equal(store.compareDelete('b',store.read(['b'])[0].generation),true); store.flush();
  put(store,'c',['C']);
  return {h,store,expected:store.read(['a','b','c'])};
}
test('physical LSM group CAS retains read guards, owned values, empty journals and tombstones',()=>{
  const h=new LsmHarness(),store=open(h);assert.equal(store.supportsDurableFlush,true);
  const initial=store.read(['a','b']);assert.deepEqual(initial,[{generation:'0',entries:[]},{generation:'0',entries:[]}]);
  assert.equal(store.compareExchange(['a','b'],initial,[['\ufeff雪\0😀\n'],[]]),true);
  const first=store.read(['a','b']);assert.equal(first[0].generation,first[1].generation);
  first[0].entries[0]='caller';assert.equal(store.read(['a'])[0].entries[0],'\ufeff雪\0😀\n');
  store.flush();h.reset();
  assert.equal(store.compareExchange(['a','b'],initial,[null,['lost']]),false);assert.deepEqual(h.trace,[]);
  const expected=store.read(['a','b']);h.reset();assert.equal(store.compareExchange(['a','b'],expected,[null,null]),true);assert.deepEqual(h.trace,[]);
  assert.equal(store.compareExchange(['a','b'],expected,[expected[0].entries,expected[1].entries]),true);assert.deepEqual(h.trace,[]);
  assert.equal(store.compareDelete('a',expected[0].generation),true);
  const deleted=store.read(['a'])[0];assert.ok(BigInt(deleted.generation)>BigInt(expected[0].generation));assert.deepEqual(deleted.entries,[]);
  h.reset();assert.equal(store.compareDelete('a',deleted.generation),false);assert.deepEqual(h.trace,[]);
  assert.deepEqual(store.scanNamespace().map(e=>e.key),['b']);
  put(store,'a',[]);store.compact();const image=h.image();store.close();assert.equal(h.leases.size,0);
  const reopened=open(new LsmHarness(image));assert.deepEqual(reopened.read(['a','b']).map(v=>v.entries),[[],[]]);assert.deepEqual(reopened.scanNamespace().map(e=>e.key),['a','b']);reopened.close();
});
test('bounded buffering, WAL replacement and full compaction continue through repeated writes',()=>{
  const h=new LsmHarness(),store=open(h);
  for(let i=0;i<30;i++){put(store,'k'+(i%5),[String(i)]);if(i%4===0)store.flush();}
  store.compact();const expected=store.scanNamespace();assert.ok(h.trace.some(s=>s.startsWith('delete:')));
  const state=LsmWal.recover(JSON.parse(h.wal.read()),selected);assert.equal(state.manifest.length,1);assert.equal(state.buffer.length,0);
  assert.ok(JSON.parse(h.wal.read()).length<=selected.maxWalRecords);assert.ok(h.tables.size<=selected.maxRuns);
  const image=h.image();store.close();const recovery=new LsmHarness(image),reopened=open(recovery);assert.deepEqual(reopened.scanNamespace(),expected);
  recovery.reset();assert.equal(reopened.read(['k2'])[0].entries[0],'27');assert.ok([...recovery.tables.values()].some(d=>d.calls.includes('readPage')));reopened.close();
});
test('every finite compaction mutation cut preserves acknowledged values and fences the old owner',()=>{
  const probe=prepared();probe.h.reset();probe.store.compact();const trace=probe.h.trace.slice();probe.store.close();
  assert.ok(trace.some(s=>s.startsWith('create:')));assert.ok(trace.some(s=>s.startsWith('delete:')));
  for(let at=1;at<=trace.length;at++)for(const after of [false,true]){
    const {h,store,expected}=prepared();h.reset();h.fault={at,after};assert.throws(()=>store.compact(),/injected/);
    const calls=h.trace.length;assert.throws(()=>store.read(['a']),/fenc|reopen|uncertain/i);assert.equal(h.trace.length,calls);
    const recovered=h.crash(),fresh=open(recovered);assert.deepEqual(fresh.read(['a','b','c']),expected,`${at}/${after}/${trace[at-1]}`);
    const reserved=BigInt(LsmWal.recover(JSON.parse(recovered.wal.read()),selected).lastReservedTableId);
    recovered.reset();put(fresh,'d',['following']);fresh.flush();
    for(const name of recovered.trace.filter(s=>s.startsWith('create:')))assert.ok(BigInt(name.slice(7))>reserved);
    fresh.close();
  }
  console.log(`compaction: ${trace.length} mutation effects; ${trace.length*2} before/after cuts`);
});
test('bad selected files and unreserved names fail before any cleanup or WAL mutation',()=>{
  const {h,store}=prepared();store.flush();const good=h.image();store.close();
  const state=LsmWal.recover(JSON.parse(h.wal.read()),selected),selectedId=state.manifest[0].id;
  for(const damage of ['missing','page','unreserved']){
    const image=structuredClone(good);
    if(damage==='missing')image.tables=image.tables.filter(([id])=>id!==selectedId);
    if(damage==='page')image.tables.find(([id])=>id===selectedId)[1].pages[0][64]^=1;
    if(damage==='unreserved')image.tables.push([(BigInt(state.lastReservedTableId)+1n).toString(),{metadata:[undefined,undefined],pages:[]}]);
    const broken=new LsmHarness(image);broken.reset();assert.throws(()=>open(broken));
    assert.deepEqual(broken.image(),image);assert.ok(broken.trace.every(s=>s.startsWith('close:')));assert.equal(broken.leases.size,0);
  }
});
test('invalid admission, stale guards and oversized groups preserve physical state',()=>{
  const h=new LsmHarness(),store=open(h),before=h.image();h.reset();
  assert.throws(()=>store.compareExchange(['a'],[{generation:'0',entries:[]}],[['x'.repeat(129)]]));
  assert.throws(()=>store.compareExchange(['a','a'],[{generation:'0',entries:[]},{generation:'0',entries:[]}],[[],[]]));
  assert.throws(()=>store.scanNamespace({maxKeys:0}));assert.deepEqual(h.trace,[]);assert.deepEqual(h.image(),before);store.close();
});
