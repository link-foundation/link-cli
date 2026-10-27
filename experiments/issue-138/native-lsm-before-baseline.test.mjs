// The native public entry must provide real owned files, not an in-memory adapter.
import assert from 'node:assert/strict';
import test from 'node:test';
import * as fs from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { FileLsmJournalStore } from './native-lsm-baseline/js/client/src/engine-file-lsm-store.js';
test('native LSM owns its namespace and reopens acknowledged journals',()=>{
 const directory=fs.mkdtempSync(join(tmpdir(),'native-lsm-before-'));
 try{
  const keys=['a','b'],store=new FileLsmJournalStore(directory,{maxPages:128});
  try{assert.equal(store.compareExchange(keys,store.read(keys),[['durable\0雪😀'],[]]),true);store.flush();assert.throws(()=>new FileLsmJournalStore(directory,{maxPages:128}));}finally{store.close();}
  const reopened=new FileLsmJournalStore(directory,{maxPages:128});try{assert.deepEqual(reopened.read(keys).map(value=>value.entries),[['durable\0雪😀'],[]]);}finally{reopened.close();}
  assert.ok(fs.readdirSync(directory).every(name=>!name.endsWith('.writer')&&name!=='.lsm-writer'));
 }finally{fs.rmSync(directory,{recursive:true,force:true});}
});
