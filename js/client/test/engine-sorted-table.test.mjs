import assert from 'node:assert/strict';
import test from 'node:test';
import { SortedJournalTable } from '../src/engine-sorted-table.js';
import { Device } from './support/sorted-table.mjs';
const rows = [{ key:'a',generation:'1',entries:['\ufeff雪\0😀\n'] },{key:'b',generation:'18446744073709551615',entries:null},{key:'c',generation:'3',entries:[]}];
test('sealed sorted table roundtrips tombstones and owns returned values',()=>{
 const device=new Device();const table=SortedJournalTable.create(device,rows);
 assert.equal(table.count,3);assert.equal(table.supportsDurableFlush,true);
 for(const row of rows) assert.deepEqual(table.get(row.key),row);
 assert.equal(table.get('absent'),undefined);assert.deepEqual(table.entries(),rows);
 table.get('a').entries[0]='caller';assert.deepEqual(table.get('a'),rows[0]);
 assert.deepEqual(SortedJournalTable.open(device.crash()).entries(),rows);
});

test('all independent sorted table vectors validate before any publication', async()=>{
 const {readFileSync}=await import('node:fs');const vectors=JSON.parse(readFileSync(new URL('../../../docs/protocol/native-sorted-table-v1.json',import.meta.url)));
 for(const fixture of vectors.cases){
  const image={metadata:fixture.metadata.map(text=>text===''?undefined:new Uint8Array(Buffer.from(text,'hex'))),pages:fixture.pages.map(text=>new Uint8Array(Buffer.from(text,'hex')))};
  const device=new Device(image);
  if(fixture.valid){const table=SortedJournalTable.open(device);assert.deepEqual(table.entries(),fixture.rows.map(([key,generation,entries])=>({key,generation,entries})));assert.equal(table.storedBytes,fixture.records.reduce((n,r)=>n+new TextEncoder().encode(r).length,0));assert.deepEqual(table.identity,Uint8Array.from({length:16},(_,i)=>i));assert.notEqual(table.digest,'0');}
  else assert.throws(()=>SortedJournalTable.open(device),undefined,fixture.name);
  assert.deepEqual(device.live,image);assert.deepEqual(device.durable,image);assert.ok(device.calls.every(call=>call==='readPage'));
 }
});
test('invalid rows and bounds perform no I/O',()=>{
 const bad=[null,{}, {key:'',generation:'1',entries:[]},{key:'a/b',generation:'1',entries:[]},{key:'a'.repeat(129),generation:'1',entries:[]},{key:1,generation:'1',entries:[]},
 ...['0','01','1\n','-1','18446744073709551616','1'.repeat(21),1].map(generation=>({key:'a',generation,entries:[]})),
 ...[{},[1],['\ud800'],Array(1)].map(entries=>({key:'a',generation:'1',entries}))];
 for(const entry of bad){const device=new Device();assert.throws(()=>SortedJournalTable.create(device,[entry]));assert.deepEqual(device.calls,[]);assert.equal(device.reads,0);}
 for(const input of [[rows[1],rows[0]],[rows[0],rows[0]],Array(1)]){const device=new Device();assert.throws(()=>SortedJournalTable.create(device,input));assert.equal(device.reads,0);}
 for(const bounds of [{maxKeys:0},{maxKeys:65537},{maxKeys:1.5},{maxBytes:0},{maxBytes:67108865},{maxBytes:1.5},{maxKeys:2},{maxBytes:1}]){
  const device=new Device();assert.throws(()=>SortedJournalTable.create(device,rows,bounds));assert.equal(device.reads,0);
 }
 const device=new Device();assert.throws(()=>SortedJournalTable.create(device,null));assert.throws(()=>SortedJournalTable.create(device,[],{maxBytes:1}));assert.equal(device.reads,0);
 assert.throws(()=>new SortedJournalTable(),/create\/open/);
});
test('point lookup reads only selected pages and preserves immutable identity',()=>{
 const input=Array.from({length:80},(_,i)=>({key:'k'+String(i).padStart(3,'0'),generation:String(i+1),entries:['x'.repeat(1000)]}));
 const device=new Device();const table=SortedJournalTable.create(device,input);assert.ok(device.live.pages.length>40);
 device.calls=[];assert.deepEqual(table.get('k040'),input[40]);assert.ok(device.calls.length<=3,device.calls.length);
 device.calls=[];assert.equal(table.get('j'),undefined);assert.equal(table.get('k999'),undefined);assert.deepEqual(device.calls,[]);
 const identity=table.identity;identity[0]^=1;assert.deepEqual(table.get('k000'),input[0]);
 assert.throws(()=>table.get('bad/name'));device.supportsDurableFlush=false;assert.equal(table.supportsDurableFlush,false);
 device.live.pages.at(-1)[64]^=1;assert.throws(()=>table.get('k079'),/checksum|identity/);assert.throws(()=>table.entries(),/checksum|identity/);
});
test('finite creation cuts never expose a partial sealed table',()=>{
 const probe=new Device();SortedJournalTable.create(probe,rows);
 for(let i=0;i<probe.calls.length;i++){if(probe.calls[i]==='readPage')continue;for(const after of [false,true]){
  const device=new Device();device.fault={at:i+1,after};assert.throws(()=>SortedJournalTable.create(device,rows),/injected/);
  const recovered=device.crash(),before=structuredClone(recovered.live);let table;
  try{table=SortedJournalTable.open(recovered);}catch(error){assert.match(error.message,/seal|decision|length/);}
  if(table!==undefined)assert.deepEqual(table.entries(),rows);assert.deepEqual(recovered.live,before);
 }}
});
test('existing data, excessive limits, torn seal and trailing pages never mutate storage',()=>{
 const device=new Device();SortedJournalTable.create(device,rows);const good=structuredClone(device.live);
 assert.throws(()=>SortedJournalTable.create(device,rows),/empty device/);assert.deepEqual(device.live,good);
 for(const bounds of [{maxBytes:8},{maxKeys:1},{maxBytes:300,maxKeys:1}])assert.throws(()=>SortedJournalTable.open(new Device(good),bounds));
 for(let length=0;length<128;length++){const image=structuredClone(good);image.metadata[1]=image.metadata[1].slice(0,length);const bad=new Device(image);assert.throws(()=>SortedJournalTable.open(bad));assert.deepEqual(bad.live,image);}
 const trailing=structuredClone(good);trailing.pages.push(new Uint8Array(4096));const bad=new Device(trailing);assert.throws(()=>SortedJournalTable.open(bad),/length/);assert.deepEqual(bad.live,trailing);
 assert.throws(()=>SortedJournalTable.open(new Device()),/decision|seal/);
 const unsealed=new Device({metadata:[new Uint8Array(),undefined],pages:[]});assert.throws(()=>SortedJournalTable.open(unsealed));
 const wrongMode=new Device(good);wrongMode.metadataFlags=2;assert.throws(()=>SortedJournalTable.open(wrongMode),/mode/);
});
test('conflicting decisions reject before repair, while identical seals and completion hooks validate',()=>{
 const probe=new Device();SortedJournalTable.create(probe,rows);const good=structuredClone(probe.live);
 const sign=(bytes)=>{let hash=0xcbf29ce484222325n;for(const byte of bytes.subarray(0,120))hash=BigInt.asUintN(64,(hash^BigInt(byte))*0x100000001b3n);new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength).setBigUint64(120,hash,true);};
 for(const conflict of ['identity','same-generation']){
  const image=structuredClone(good);
  if(conflict==='identity')image.metadata[0][16]^=1;
  else {image.metadata[0]=image.metadata[1].slice();image.metadata[0][72]^=1;}
  sign(image.metadata[0]);const device=new Device(image);let complete=0;device.completeRecovery=()=>{complete++;};
  assert.throws(()=>SortedJournalTable.open(device),/conflicting/);assert.equal(complete,0);assert.deepEqual(device.live,image);assert.deepEqual(device.calls,[]);
 }
 const image=structuredClone(good);image.metadata[0]=image.metadata[1].slice();const device=new Device(image);let complete=0;
 device.completeRecovery=()=>{complete++;};assert.deepEqual(SortedJournalTable.open(device).entries(),rows);assert.equal(complete,1);
 device.completeRecovery=()=>Promise.resolve();assert.throws(()=>SortedJournalTable.open(device),/synchronously/);assert.deepEqual(device.live,image);
});
test('selected mirrored table repairs only after complete validation',async()=>{
 const {MirroredPageDevice}=await import('../src/engine-mirrored-pages.js');const first=new Device(),second=new Device();
 SortedJournalTable.create(new MirroredPageDevice(first,second),rows);
 first.live.pages[0][64]^=1;first.live.metadata[1]=first.live.metadata[1].slice(0,17);
 assert.deepEqual(SortedJournalTable.open(new MirroredPageDevice(first,second)).entries(),rows);assert.deepEqual(first.live,second.live);
 first.live.pages[0][64]^=1;second.live.pages[0][64]^=2;const a=structuredClone(first.live),b=structuredClone(second.live);
 assert.throws(()=>SortedJournalTable.open(new MirroredPageDevice(first,second)),/neither/);assert.deepEqual(first.live,a);assert.deepEqual(second.live,b);
});
