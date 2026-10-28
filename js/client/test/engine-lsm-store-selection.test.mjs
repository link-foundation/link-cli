import assert from 'node:assert/strict';
import test from 'node:test';
import { PhysicalLsmJournalStore as Store } from '../src/engine-lsm-store.js';
import { LsmWal } from '../src/engine-lsm-wal.js';
import { SortedJournalTable as Table } from '../src/engine-sorted-table.js';
import { LsmHarness, selected } from './support/lsm-store.mjs';
const open=(h,options=selected)=>Store.open(h.wal,h.provider,options);
const put=(s,k,v)=>assert.equal(s.compareExchange([k],s.read([k]),[v]),true);
function retained(rows,generation,options=selected) {
  const h=new LsmHarness(),manifest=[];
  for(const entries of rows) {
    const id=String(manifest.length+1),device=h.provider.create(id),table=Table.create(device,entries);
    manifest.push({id,identity:[...table.identity].map(v=>v.toString(16).padStart(2,'0')).join(''),digest:table.digest,count:table.count,storedBytes:table.storedBytes});
    h.provider.close(device);
  }
  h.wal.replace([LsmWal.encode({generation,coveredGeneration:generation,lastReservedTableId:String(manifest.length),manifest,buffer:[]},options)]);
  h.reset();return h;
}
test('selected table generations and combined retained capacity require physical validation',()=>{
  for(const [rows,generation,options,message] of [
    [[[ {key:'a',generation:'2',entries:[]} ]],'1',selected,/exceeds coverage/],
    [[[ {key:'a',generation:'1',entries:[]} ]],'2',selected,/missing the global generation/],
    [[[ {key:'a',generation:'1',entries:[]} ],[ {key:'b',generation:'2',entries:[]} ]],'2',{...selected,maxKeys:1,bufferKeys:1},/retained capacity/],
    [[[ {key:'a',generation:'1',entries:['A']} ],[ {key:'b',generation:'2',entries:['B']} ]],'2',{...selected,maxBytes:20,bufferBytes:20},/retained capacity/],
  ]) {
    const h=retained(rows,generation,options),before=h.image();
    assert.throws(()=>open(h,options),message);assert.deepEqual(h.image(),before);assert.equal(h.leases.size,0);
    assert.ok(h.trace.every(name=>name.startsWith('close:')));
  }
});
test('point lookup detects a table reader returning a changed indexed value',()=>{
  for(const changed of [undefined,{key:'a',generation:'2',entries:['A']},{key:'a',generation:'1',entries:null},{key:'a',generation:'1',entries:['different cost']}]) {
    const h=retained([[{key:'a',generation:'1',entries:['A']}]],'1'),s=open(h),original=Table.prototype.get;
    try { Table.prototype.get=()=>changed;assert.throws(()=>s.read(['a']),/point lookup changed/); }
    finally {Table.prototype.get=original;s.close();}
  }
});
test('a table factory returning different preflight bounds cannot publish a manifest',()=>{
  for(const changed of ['count','storedBytes']) {
    const h=new LsmHarness(),s=open(h);put(s,'a',['A']);const original=Table.create;
    try {
      Table.create=(...args)=>{
        const table=original(...args);
        return {count:table.count+(changed==='count'?1:0),storedBytes:table.storedBytes+(changed==='storedBytes'?1:0),identity:table.identity,digest:table.digest};
      };
      assert.throws(()=>s.flush(),/prepared table bounds changed/);assert.throws(()=>s.read(['a']),/fenced/);
    } finally {Table.create=original;s.close();}
    const fresh=h.crash(),recovered=open(fresh);assert.deepEqual(recovered.read(['a'])[0].entries,['A']);
    recovered.flush();assert.deepEqual([...fresh.tables.keys()],['2']);recovered.close();
  }
});
test('a recovery completion with a result rejects and releases the selected lease',()=>{
  const h=retained([[{key:'a',generation:'1',entries:['A']}]],'1');
  [...h.tables.values()][0].completeRecovery=()=>true;h.reset();
  assert.throws(()=>open(h),/without a result/);assert.equal(h.leases.size,0);
});
