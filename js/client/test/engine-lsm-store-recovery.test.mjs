import assert from 'node:assert/strict';
import test from 'node:test';
import { PhysicalLsmJournalStore } from '../src/engine-lsm-store.js';
import { LsmWal } from '../src/engine-lsm-wal.js';
import { LsmHarness, selected } from './support/lsm-store.mjs';
const open=h=>PhysicalLsmJournalStore.open(h.wal,h.provider,selected);
function sealed() {
  const h=new LsmHarness(),s=open(h);
  for(const key of ['a','b']){s.compareExchange([key],s.read([key]),[[key]]);s.flush();}
  s.close();return h.image();
}
test('all selected table bindings validate before any staged repair completes',()=>{
  const image=sealed(),broken=new LsmHarness(image),state=LsmWal.recover(JSON.parse(broken.wal.read()),selected);
  state.manifest[1].digest=state.manifest[1].digest==='0'?'1':'0';
  broken.wal.replace([LsmWal.encode(state,selected)]);
  const before=broken.image();let repairs=0;
  for(const device of broken.tables.values())device.completeRecovery=()=>{repairs++;};
  broken.reset();assert.throws(()=>open(broken),/binding changed/);
  assert.equal(repairs,0);assert.deepEqual(broken.image(),before);assert.equal(broken.leases.size,0);
  const good=new LsmHarness(image);let completed=0;
  for(const device of good.tables.values())device.completeRecovery=()=>{
    assert.equal(good.reads.filter(name=>name.startsWith('open:')).length,2);
    completed++;
  };
  const s=open(good);assert.equal(completed,2);assert.deepEqual(s.read(['a','b']).map(v=>v.entries),[['a'],['b']]);s.close();
});
test('failed explicit close retains cleanup ownership and permits close retry',()=>{
  const h=new LsmHarness(sealed()),s=open(h),close=h.provider.close;let fail=true;
  h.provider.close=device=>{if(fail){fail=false;throw Error('close admission failed');}return close(device);};
  assert.throws(()=>s.close(),AggregateError);assert.equal(h.leases.size,1);
  assert.throws(()=>s.read(['a']),/closed/);
  s.close();assert.equal(h.leases.size,0);s.close();
});
