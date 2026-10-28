import assert from 'node:assert/strict';
import test from 'node:test';
import * as fs from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { FileLsmJournalStore } from '../src/engine-file-lsm-store.js';
import { selected } from './support/lsm-store.mjs';
const phases=['open','group','flush','following','compact'];
const snapshot=directory=>Object.fromEntries(fs.readdirSync(directory).sort().map(name=>[name,createHash('sha256').update(fs.readFileSync(join(directory,name))).digest('hex')]));
test('ten finite abrupt children retain exclusive fences and acknowledged state through native recovery',()=>{
 for(const mirrored of [false,true])for(const stop of phases){
  const directory=fs.mkdtempSync(join(tmpdir(),'lsm-native-exit-'));
  try{
   const child=spawnSync(process.execPath,[fileURLToPath(new URL('./support/file-lsm-process.mjs', import.meta.url)),directory,stop,mirrored?'1':'0'],{encoding:'utf8',maxBuffer:4096});
   assert.equal(child.error,undefined);assert.equal(child.signal,null);assert.equal(child.status,73,child.stderr);assert.equal(child.stdout,phases.slice(0,phases.indexOf(stop)+1).join('\n')+'\n');
   const options={...selected,maxPages:32,mirroredRoot:mirrored,mirroredBanks:mirrored,mirroredTables:mirrored};
   const before=snapshot(directory);assert.throws(()=>new FileLsmJournalStore(directory,options),/EEXIST/);assert.deepEqual(snapshot(directory),before);
   // The child has exited and been reaped. This explicit external recovery step
   // removes only its recognized owner markers; production open never steals.
   for(const name of fs.readdirSync(directory))if(name==='.lsm-writer'||name.endsWith('.writer'))fs.unlinkSync(join(directory,name));
   const store=new FileLsmJournalStore(directory,options),values=store.read(['a','b']);
   assert.deepEqual(values.map(value=>value.entries),stop==='open'?[[],[]]:[['following','compact'].includes(stop)?'following':'durable\0雪😀'].map(value=>[value]).concat([[]]));
   if(stop!=='open')assert.equal(values[1].generation,'1');
   const generation=BigInt(values[0].generation);assert.equal(store.compareExchange(['a'],values.slice(0,1),[['after restart']]),true);assert.ok(BigInt(store.read(['a'])[0].generation)>generation);
   store.flush();store.close();assert.ok(fs.readdirSync(directory).every(name=>name!=='.lsm-writer'&&!name.endsWith('.writer')));
   const again=new FileLsmJournalStore(directory,options);assert.deepEqual(again.read(['a'])[0].entries,['after restart']);again.close();
  }finally{fs.rmSync(directory,{recursive:true,force:true});}
 }
});
