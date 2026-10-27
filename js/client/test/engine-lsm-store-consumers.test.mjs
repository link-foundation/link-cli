import assert from 'node:assert/strict';
import test from 'node:test';
import { PhysicalLsmJournalStore as Store } from '../src/engine-lsm-store.js';
import { ClusterEngine, PartitionedJournalStore, ReclaimingPartitionedJournalStore } from '../src/index.js';
import { LsmHarness, selected } from './support/lsm-store.mjs';
const options={...selected,maxKeys:128,maxBytes:65536,bufferKeys:32,bufferBytes:16384,maxWalBytes:262144};
const open=h=>Store.open(h.wal,h.provider,options);
function providers(images) {
  const names=['catalog','east','west'];
  return Object.fromEntries(names.map(name=>{const harness=new LsmHarness(images?.[name]);return [name,{harness,store:open(harness)}];}));
}
function checkpoint(p) {
  const images={};
  for(const [name,{harness,store}] of Object.entries(p)){store.compact();images[name]=harness.image();store.close();}
  return images;
}
function routed(p,collect=false) {
  const args=[p.catalog.store,{east:p.east.store,west:p.west.store},key=>key==='a'?'east':'west'];
  return collect?ReclaimingPartitionedJournalStore.open(...args):new PartitionedJournalStore(...args);
}
test('partitioned catalogs accept synchronous physical stores and survive compaction/reopen',async()=>{
  const p=providers(),s=routed(p),initial=await s.read(['a','b']);
  assert.equal(s.supportsDurableFlush,true);assert.equal(await s.compareExchange(['a','b'],initial,[['雪'],[]]),true);
  const before=await s.read(['a','b']);assert.equal(await s.compareExchange(['a'],[before[0]],[['new']]),true);
  assert.equal(await s.compareExchange(['a','b'],before,[null,['stale']]),false);
  const image=checkpoint(p),fresh=providers(image),reopened=routed(fresh);
  assert.deepEqual((await reopened.read(['a','b'])).map(row=>row.entries),[['new'],[]]);checkpoint(fresh);
});
test('epoch-fenced collection composes synchronous physical catalogs, scans and tombstone deletion',async()=>{
  const p=providers(),s=await routed(p,true);
  assert.equal(s.supportsDurableFlush,true);
  for(const i of [0,1])assert.equal(await s.compareExchange(['a','b'],await s.read(['a','b']),[['a'+i],['b'+i]]),true);
  assert.deepEqual(await s.collectGarbage(),{epoch:'1',examined:4,deleted:2});
  assert.deepEqual((await s.read(['a','b'])).map(row=>row.entries),[['a1'],['b1']]);
  const image=checkpoint(p),fresh=providers(image),reopened=await routed(fresh,true);
  assert.deepEqual((await reopened.read(['a','b'])).map(row=>row.entries),[['a1'],['b1']]);
  assert.equal((await reopened.collectGarbage()).deleted,0);checkpoint(fresh);
});
test('cluster adapter awaits synchronous group CAS and retains stale read guards',async()=>{
  const h=new LsmHarness(),s=open(h);let next=0;const released=[];
  const binding={StagedEngine:class {
    constructor(_options,journal){this.records=JSON.parse(journal);this.id=next++;}
    begin(){return {request:()=>'(count: 0)',commit:()=>{this.records=['changed-'+this.id];},rollback(){},free:()=>released.push('session'+this.id)};}
    journal(){return JSON.stringify(this.records);}
    free(){released.push('engine'+this.id);}
  }};
  const engine=new ClusterEngine(binding,s,{requireDurableFlush:true});
  const stale=await engine.begin(['a','b']),winner=await engine.begin(['a']);await winner.commit();await winner.close();
  await assert.rejects(stale.commit(),{outcome:'failed'});await assert.rejects(stale.close(),{outcome:'failed'});
  assert.deepEqual(s.read(['a','b']).map(row=>row.entries),[['changed-2'],[]]);
  assert.equal(released.length,6);s.compact();s.close();
  const fresh=open(new LsmHarness(h.image()));assert.deepEqual(fresh.read(['a'])[0].entries,['changed-2']);fresh.close();
});
