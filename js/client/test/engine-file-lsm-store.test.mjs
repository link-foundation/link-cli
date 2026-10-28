// Native file composition controls; finite files and no physical power-loss claim.
import assert from 'node:assert/strict';
import test from 'node:test';
import * as fs from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { FileLsmJournalStore } from '../src/engine-file-lsm-store.js';
import { selected } from './support/lsm-store.mjs';
import { selectDecision } from '../src/engine-checkpoint-page-format.js';
import { decodeMetadata } from '../src/engine-page-format.js';
const options={...selected,maxPages:32};
const put=(store,key,value)=>assert.equal(store.compareExchange([key],store.read([key]),[value]),true);
const temp=()=>fs.mkdtempSync(join(tmpdir(),'clink-file-lsm-'));
const files=directory=>fs.readdirSync(directory).filter(name=>!name.endsWith('.writer')&&name!=='.lsm-writer').sort();
const snapshot=directory=>Object.fromEntries(files(directory).map(name=>[name,createHash('sha256').update(fs.readFileSync(join(directory,name))).digest('hex')]));
const ownerFree=directory=>assert.ok(fs.readdirSync(directory).every(name=>!name.endsWith('.writer')&&name!=='.lsm-writer'));
function fixture(directory,modes={}){const store=new FileLsmJournalStore(directory,{...options,...modes});put(store,'a',['old\0雪😀']);put(store,'b',[]);store.flush();put(store,'a',['selected']);store.flush();put(store,'c',['buffered']);const expected=store.scanNamespace();store.close();return expected;}
function selectedBank(directory){const raw=[0,1].map(i=>fs.readFileSync(join(directory,'wal.meta'+i)));return Number(selectDecision(raw,0).epoch%2n);}
function run(body){const directory=temp();try{body(directory);}finally{fs.rmSync(directory,{recursive:true,force:true});}}

test('all eight selected mirror combinations reopen actual files, own values and compact',()=>run(directory=>{
 for(let bits=0;bits<8;bits++){
  const path=join(directory,String(bits));fs.mkdirSync(path);const modes={mirroredRoot:!!(bits&1),mirroredBanks:!!(bits&2),mirroredTables:!!(bits&4)};
  const expected=fixture(path,modes),before=snapshot(path),store=new FileLsmJournalStore(path,{...options,...modes});
  assert.deepEqual(store.scanNamespace(),expected);assert.equal(store.supportsDurableFlush,process.platform!=='win32');
  const values=store.read(['a']);values[0].entries.push('caller');assert.deepEqual(store.read(['a'])[0].entries,['selected']);
  assert.equal(store.compareExchange(['a'],[{generation:'0'}],[['lost']]),false);
  store.close();ownerFree(path);assert.deepEqual(snapshot(path),before,'read-only reopen preserves every physical byte');
  const writer=new FileLsmJournalStore(path,{...options,...modes});writer.compact();put(writer,'d',['following']);writer.flush();const after=writer.scanNamespace();writer.close();ownerFree(path);
  const reader=new FileLsmJournalStore(path,{...options,...modes});assert.deepEqual(reader.scanNamespace(),after);reader.close();ownerFree(path);
 }
}));

test('missing selected table data rejects before file creation, WAL suffix truncation or orphan retirement',()=>run(directory=>{
 fixture(directory);const bank=selectedBank(directory);fs.appendFileSync(join(directory,'wal.bank'+bank),Buffer.alloc(4096,17));
 const table=files(directory).find(name=>/^table-[0-9]+$/.test(name));fs.unlinkSync(join(directory,table));const before=snapshot(directory);
 assert.throws(()=>new FileLsmJournalStore(directory,options));assert.deepEqual(snapshot(directory),before);ownerFree(directory);
}));

test('corrupt selected table rejects before checkpoint suffix removal',()=>run(directory=>{
 fixture(directory);const bank=selectedBank(directory);fs.appendFileSync(join(directory,'wal.bank'+bank),Buffer.alloc(4096,18));
 const table=join(directory,files(directory).find(name=>/^table-[0-9]+$/.test(name))),data=fs.readFileSync(table);data[100]^=1;fs.writeFileSync(table,data);const before=snapshot(directory);
 assert.throws(()=>new FileLsmJournalStore(directory,options));assert.deepEqual(snapshot(directory),before);ownerFree(directory);
}));

test('outer fence prevents every competing owner before acquiring child files',()=>run(directory=>{
 const first=new FileLsmJournalStore(directory,options),before=snapshot(directory);
 assert.throws(()=>new FileLsmJournalStore(directory,options),/EEXIST/);assert.deepEqual(snapshot(directory),before);assert.ok(fs.existsSync(join(directory,'.lsm-writer')));
 put(first,'a',['survives']);first.close();ownerFree(directory);
 fs.writeFileSync(join(directory,'.lsm-writer'),'stale evidence');const closed=snapshot(directory);
 assert.throws(()=>new FileLsmJournalStore(directory,options),/EEXIST/);assert.deepEqual(snapshot(directory),closed);assert.equal(fs.readFileSync(join(directory,'.lsm-writer'),'utf8'),'stale evidence');
}));

test('foreign, malformed, unreserved, symbolic and stale child names preserve evidence',()=>{
 for(const name of ['foreign','table-01','table-18446744073709551616','table-999','wal.bank0.writer','table-1.mirror'])run(directory=>{
  fixture(directory);fs.writeFileSync(join(directory,name),'evidence');const before=snapshot(directory);assert.throws(()=>new FileLsmJournalStore(directory,options));assert.deepEqual(snapshot(directory),before);assert.ok(fs.existsSync(join(directory,name)));assert.equal(fs.existsSync(join(directory,'.lsm-writer')),false);
 });
 run(directory=>{fixture(directory);fs.symlinkSync(join(directory,'wal'),join(directory,'symbolic'));const before=snapshot(directory);assert.throws(()=>new FileLsmJournalStore(directory,options));assert.deepEqual(snapshot(directory),before);assert.ok(fs.lstatSync(join(directory,'symbolic')).isSymbolicLink());ownerFree(directory);});
});

test('invalid finite options reject before acquiring ownership',()=>run(directory=>{
 for(const selectedOptions of [{maxPages:0},{maxPages:65537},{maxNamespaceEntries:1},{maxNamespaceEntries:65537},{mirroredRoot:1},{bufferKeys:0}]){
  assert.throws(()=>new FileLsmJournalStore(directory,{...options,...selectedOptions}));assert.deepEqual(fs.readdirSync(directory),[]);
 }
}));

test('nonempty unrooted namespace is rejected without initialization',()=>run(directory=>{
 fs.writeFileSync(join(directory,'table-1'),'orphan');const before=snapshot(directory);assert.throws(()=>new FileLsmJournalStore(directory,options));assert.deepEqual(snapshot(directory),before);ownerFree(directory);
}));

test('a valid selected suffix is truncated only after complete table validation',()=>run(directory=>{
 const expected=fixture(directory),bank=selectedBank(directory),path=join(directory,'wal.bank'+bank),length=fs.statSync(path).size;
 fs.appendFileSync(path,Buffer.alloc(4096,19));const store=new FileLsmJournalStore(directory,options);assert.deepEqual(store.scanNamespace(),expected);assert.equal(fs.statSync(path).size,length);store.close();ownerFree(directory);
}));

test('mirror repair waits for every selected table binding to validate',()=>run(directory=>{
 const modes={mirroredRoot:true,mirroredBanks:true,mirroredTables:true};fixture(directory,modes);
 const tables=files(directory).filter(name=>/^table-[0-9]+$/.test(name));assert.ok(tables.length>=2);
 const first=join(directory,tables[0]),damage=fs.readFileSync(first);damage[100]^=1;fs.writeFileSync(first,damage);
 const later=join(directory,tables[1]+'.meta0'),original=fs.readFileSync(later);fs.writeFileSync(later,Buffer.alloc(128,22));
 const laterSecond=join(directory,tables[1]+'.meta1'),originalSecond=fs.readFileSync(laterSecond);fs.writeFileSync(laterSecond,Buffer.alloc(128,22));
 const laterMirror=join(directory,tables[1]+'.mirror.meta0'),originalMirror=fs.readFileSync(laterMirror);fs.writeFileSync(laterMirror,Buffer.alloc(128,22));
 const laterMirrorSecond=join(directory,tables[1]+'.mirror.meta1'),originalMirrorSecond=fs.readFileSync(laterMirrorSecond);fs.writeFileSync(laterMirrorSecond,Buffer.alloc(128,22));
 const before=snapshot(directory);assert.throws(()=>new FileLsmJournalStore(directory,{...options,...modes}));assert.deepEqual(snapshot(directory),before);ownerFree(directory);
 for(const [path,bytes] of [[later,original],[laterSecond,originalSecond],[laterMirror,originalMirror],[laterMirrorSecond,originalMirrorSecond]])fs.writeFileSync(path,bytes);
 const store=new FileLsmJournalStore(directory,{...options,...modes});assert.equal(store.read(['a'])[0].entries[0],'selected');store.close();assert.deepEqual(fs.readFileSync(first),fs.readFileSync(first+'.mirror'));ownerFree(directory);
}));

test('complete surviving copies admit missing redundant files only after global validation',()=>run(directory=>{
 const modes={mirroredRoot:true,mirroredBanks:true,mirroredTables:true},expected=fixture(directory,modes);
 const tables=files(directory).filter(name=>/^table-[0-9]+$/.test(name));assert.ok(tables.length>=2);
 const missing=[tables[0],tables[0]+'.meta0',tables[0]+'.meta1','wal.meta0','wal.meta1'];
 for(const name of missing)fs.unlinkSync(join(directory,name));
 const invalid=join(directory,tables[1]+'.mirror'),original=fs.readFileSync(invalid),damaged=Uint8Array.from(original);
 damaged[100]^=1;fs.writeFileSync(invalid,damaged);
 const primary=join(directory,tables[1]),originalPrimary=fs.readFileSync(primary),damagedPrimary=Uint8Array.from(originalPrimary);
 damagedPrimary[100]^=1;fs.writeFileSync(primary,damagedPrimary);
 const before=snapshot(directory);assert.throws(()=>new FileLsmJournalStore(directory,{...options,...modes}));
 assert.deepEqual(snapshot(directory),before);for(const name of missing)assert.equal(fs.existsSync(join(directory,name)),false);ownerFree(directory);
 fs.writeFileSync(invalid,original);fs.writeFileSync(primary,originalPrimary);
 const store=new FileLsmJournalStore(directory,{...options,...modes});assert.deepEqual(store.scanNamespace(),expected);store.close();
 for(const name of missing)assert.equal(fs.existsSync(join(directory,name)),true);ownerFree(directory);
 assert.deepEqual(fs.readFileSync(join(directory,tables[0])),fs.readFileSync(join(directory,tables[0]+'.mirror')));
}));

test('a surviving plain table metadata slot remains sufficient',()=>run(directory=>{
 const expected=fixture(directory),table=files(directory).find(name=>/^table-[0-9]+$/.test(name));
 const a=join(directory,table+'.meta0'),b=join(directory,table+'.meta1');
 const state=decodeMetadata(fs.readFileSync(a),0),keep=state?.pages>0?a:b,remove=keep===a?b:a;fs.unlinkSync(remove);
 const store=new FileLsmJournalStore(directory,options);assert.deepEqual(store.scanNamespace(),expected);store.close();ownerFree(directory);
}));
