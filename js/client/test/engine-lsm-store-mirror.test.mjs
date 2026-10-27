import assert from 'node:assert/strict';
import test from 'node:test';
import { PhysicalLsmJournalStore as Store } from '../src/engine-lsm-store.js';
import { MirroredPageDevice } from '../src/engine-mirrored-pages.js';
import { LsmWal } from '../src/engine-lsm-wal.js';
import { Device } from './support/sorted-table.mjs';
import { LsmHarness, selected } from './support/lsm-store.mjs';
function namespace() {
  const files=new Map(),leases=new Set();
  return {files,leases,supportsDurableFlush:true,
    list:max=>{assert.ok(files.size<=max);return [...files.keys()].sort((a,b)=>BigInt(a)<BigInt(b)?-1:1);},
    create(id){assert.equal(files.has(id),false);files.set(id,[new Device(),new Device()]);return this.open(id);},
    open(id){const pair=files.get(id);assert.ok(pair);const device=new MirroredPageDevice(...pair);leases.add(device);return device;},
    close:device=>{leases.delete(device);},delete:id=>{assert.equal(files.delete(id),true);}
  };
}
test('physical store composes independent mirrored table pages and repairs a surviving copy',()=>{
  const h=new LsmHarness(),p=namespace(),s=Store.open(h.wal,p,selected);
  assert.equal(s.compareExchange(['a'],s.read(['a']),[['雪']]),true);s.flush();
  assert.deepEqual(s.read(['a'])[0].entries,['雪']);s.close();assert.equal(p.leases.size,0);
  const pair=[...p.files.values()][0];pair[0].live.pages[0][0]^=1;
  const reopened=Store.open(h.wal,p,selected);assert.deepEqual(reopened.read(['a'])[0].entries,['雪']);
  assert.deepEqual(pair[0].live,pair[1].live);reopened.close();
});
test('a later bad binding prevents staged mirrored repairs in earlier selected tables',()=>{
  const h=new LsmHarness(),p=namespace(),s=Store.open(h.wal,p,selected);
  for(const key of ['a','b']){assert.equal(s.compareExchange([key],s.read([key]),[[key]]),true);s.flush();}s.close();
  const first=p.files.get('1');first[0].live.pages[0][0]^=1;
  const before=structuredClone(first[0].live),state=LsmWal.recover(JSON.parse(h.wal.read()),selected);
  state.manifest[1].digest=(BigInt(state.manifest[1].digest)^1n).toString();h.wal.replace([LsmWal.encode(state,selected)]);
  assert.throws(()=>Store.open(h.wal,p,selected),/binding changed/);assert.deepEqual(first[0].live,before);assert.equal(p.leases.size,0);
});
