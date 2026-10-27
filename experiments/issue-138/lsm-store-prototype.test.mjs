// Additional design controls, separate from preserved missing-production tests.
import assert from 'node:assert/strict';
import test from 'node:test';
import { PhysicalLsmJournalStore } from './lsm-store-prototype.mjs';
import { LsmWal } from '../../js/client/src/engine-lsm-wal.js';
import { SortedJournalTable } from '../../js/client/src/engine-sorted-table.js';
import { LsmHarness, selected } from './lsm-store-harness.mjs';
const open=(h,options=selected)=>PhysicalLsmJournalStore.open(h.wal,h.provider,options);
const put=(s,k,v)=>assert.equal(s.compareExchange([k],s.read([k]),[v]),true);
function prepared(){
  const h=new LsmHarness(),s=open(h);
  put(s,'a',['A']);put(s,'b',[]);s.flush();
  put(s,'a',['new']);s.compareDelete('b',s.read(['b'])[0].generation);s.flush();put(s,'c',['C']);
  const keys=['a','b','c','d'],before=s.read(keys),updates=[['changed'],['D']];
  return {h,s,keys,before,change:()=>s.compareExchange(['a','d'],[before[0],before[3]],updates)};
}
test('every group-write mutation cut recovers the whole old or whole new group',()=>{
  const probe=prepared();probe.h.reset();assert.equal(probe.change(),true);const trace=probe.h.trace.slice(),after=probe.s.read(probe.keys);probe.s.close();
  assert.ok(trace.some(name=>name.startsWith('create:')));assert.ok(trace.some(name=>name.startsWith('delete:')));
  for(let at=1;at<=trace.length;at++)for(const lostReply of [false,true]){
    const {h,s,keys,before,change}=prepared();h.reset();h.fault={at,after:lostReply};assert.throws(change,/injected/);
    const calls=h.trace.length;assert.throws(()=>put(s,'later',['no retry']),/fenced/);assert.equal(h.trace.length,calls);
    const fresh=h.crash(),recovered=open(fresh),actual=recovered.read(keys);
    assert.ok(JSON.stringify(actual)===JSON.stringify(before)||JSON.stringify(actual)===JSON.stringify(after),`${at}/${lostReply}/${trace[at-1]}`);
    put(recovered,'following',['ok']);recovered.compact();const image=fresh.image();recovered.close();
    const following=open(new LsmHarness(image));assert.equal(following.read(['following'])[0].entries[0],'ok');following.close();
  }
  console.log(`group write with compaction: ${trace.length} mutation effects; ${trace.length*2} before/after cuts`);
});
test('manifest and following group admission both precede maintenance effects',()=>{
  const h=new LsmHarness(),s=open(h,{...selected,bufferKeys:1,maxWalBytes:220});put(s,'a',['A']);
  const before=h.image();h.reset();assert.throws(()=>put(s,'b',['x'.repeat(90)]),/WAL admission/);
  assert.deepEqual(h.trace,[]);assert.deepEqual(h.image(),before);assert.deepEqual(s.read(['a','b']).map(v=>v.entries),[['A'],[]]);s.close();
});
test('counter exhaustion rejects only changed publications before mutation',()=>{
  const maximum='18446744073709551615',h=new LsmHarness();
  h.wal.replace([LsmWal.encode({generation:maximum,coveredGeneration:'0',lastReservedTableId:maximum,manifest:[],buffer:[{key:'a',generation:maximum,entries:['last']}]},selected)]);
  const s=open(h);h.reset();assert.equal(s.compareExchange(['a'],s.read(['a']),[['last']]),true);
  assert.throws(()=>put(s,'a',['overflow']),/generation exhausted/);assert.throws(()=>s.flush(),/identity exhausted/);
  assert.deepEqual(h.trace,[]);assert.equal(s.read(['a'])[0].generation,maximum);s.close();
});
test('constructor failures close acquired leases and preserve selected bytes',()=>{
  const {h,s}=prepared();s.compact();const good=h.image();s.close();
  const broken=new LsmHarness(good),device=[...broken.tables.values()][0],before=broken.image();
  device.readPage=()=>{throw Error('injected read');};broken.reset();assert.throws(()=>open(broken),/injected read/);
  assert.deepEqual(broken.image(),before);assert.equal(broken.leases.size,0);assert.ok(broken.trace.every(name=>name.startsWith('close:')));
  const cleanup=new LsmHarness(good),original=cleanup.provider.close;
  [...cleanup.tables.values()][0].readPage=()=>{throw Error('injected read');};
  cleanup.provider.close=device=>{original(device);throw Error('lost close reply');};
  assert.throws(()=>open(cleanup),error=>error instanceof AggregateError&&error.errors.length===2);
});
test('foreign namespace, thenable providers and synchronous reentry are rejected',()=>{
  const invalid=new LsmHarness();invalid.provider.list=()=>['01'];invalid.reset();assert.throws(()=>open(invalid));assert.deepEqual(invalid.trace,[]);
  const promise=new LsmHarness();promise.provider.list=()=>({then(){}});promise.reset();assert.throws(()=>open(promise),/synchronously/);assert.deepEqual(promise.trace,[]);
  const h=new LsmHarness(),s=open(h);put(s,'a',['A']);h.reset();
  const create=h.provider.create;h.provider.create=id=>{assert.throws(()=>s.read(['a']),/reentry|fenced/);return create(id);};
  s.flush();assert.equal(s.read(['a'])[0].entries[0],'A');s.close();
});
function retainedVersions(last) {
  const h=new LsmHarness(),manifest=[],options={...selected,maxRuns:3};
  for(const [i,entries] of [['old'],['latest'],[last]].entries()) {
    const id=String(i+1),device=h.provider.create(id);
    const table=SortedJournalTable.create(device,[{key:'a',generation:i===1?'2':'1',entries}]);
    manifest.push({id,identity:[...table.identity].map(v=>v.toString(16).padStart(2,'0')).join(''),digest:table.digest,count:table.count,storedBytes:table.storedBytes});
    h.provider.close(device);
  }
  const orphan=h.provider.create('4');h.provider.close(orphan);
  h.wal.replace([LsmWal.encode({generation:'2',coveredGeneration:'2',lastReservedTableId:'4',manifest,buffer:[]},options)]);
  h.reset();return {h,options};
}
test('conflicting retained historical versions reject even beneath a newer value',()=>{
  const {h,options}=retainedVersions('different'),before=h.image();
  assert.throws(()=>open(h,options),/conflicting LSM table versions/);
  assert.deepEqual(h.image(),before);assert.equal(h.leases.size,0);
  assert.ok(h.trace.every(name=>name.startsWith('close:')));assert.ok(h.tables.has('4'));
});
test('equal retained historical versions remain valid beneath a newer value',()=>{
  const {h,options}=retainedVersions('old'),s=open(h,options);
  assert.equal(s.read(['a'])[0].entries[0],'latest');assert.equal(h.tables.has('4'),false);s.close();
});
test('durable namespace capability cannot silently downgrade after publication',()=>{
  const h=new LsmHarness(),s=open(h);put(s,'a',['acknowledged']);
  assert.equal(s.supportsDurableFlush,true);
  const create=h.provider.create;
  h.provider.create=id=>{const device=create(id);device.supportsDurableFlush=false;return device;};
  h.reset();assert.throws(()=>s.flush(),/durable namespace returned a non-durable/);
  assert.equal(s.supportsDurableFlush,true);assert.throws(()=>s.read(['a']),/fenced/);s.close();assert.equal(h.leases.size,0);
  const fresh=h.crash(),recovered=open(fresh);
  assert.equal(recovered.read(['a'])[0].entries[0],'acknowledged');recovered.flush();
  assert.deepEqual([...fresh.tables.keys()],['2']);recovered.close();
  const selectedDevice=new LsmHarness(fresh.image());[...selectedDevice.tables.values()][0].supportsDurableFlush=false;
  selectedDevice.reset();assert.throws(()=>open(selectedDevice),/durable namespace returned a non-durable/);
  assert.equal(selectedDevice.leases.size,0);assert.ok(selectedDevice.trace.every(name=>name.startsWith('close:')));
});
