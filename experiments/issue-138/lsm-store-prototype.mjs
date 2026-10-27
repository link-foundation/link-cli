// Isolated physical-composition experiment. Not a production entry point.
import { LsmWal, lsmOptions } from '../../js/client/src/engine-lsm-wal.js';
import { SortedJournalTable } from '../../js/client/src/engine-sorted-table.js';
import { scanOptions } from '../../js/client/src/cluster-maintenance.js';
const encoder = new TextEncoder(), maximum = 18446744073709551615n;
const bytes = text => encoder.encode(text).length;
const cost = row => row.key.length + 8 + (row.entries ?? []).reduce((n, item) => n + bytes(item) + 1, 0);
const ordered = rows => [...rows].sort((a,b) => a.key < b.key ? -1 : 1);
const wire = rows => rows.map(row => [row.key,row.generation,row.entries]);
const equal = (a,b) => a === null || b === null ? a === b : a.length === b.length && a.every((entry,i) => entry === b[i]);
const identity = table => [...table.identity].map(byte => byte.toString(16).padStart(2,'0')).join('');
function sync(value) { if (value && typeof value.then === 'function') throw TypeError('LSM I/O must complete synchronously'); return value; }
function done(value) { if (value !== undefined) throw TypeError('LSM operation must complete synchronously without a result'); }
function counter(value, positive=false) {
  if (typeof value !== 'string' || value.length === 0 || value.length > 20 || /[^0-9]/.test(value)
    || (value.length > 1 && value[0] === '0') || BigInt(value) > maximum || (positive && value === '0')) throw TypeError('canonical LSM generation');
  return BigInt(value);
}
function keysOf(keys) {
  if (!Array.isArray(keys) || keys.length === 0 || keys.length > 32 || new Set(keys).size !== keys.length) throw TypeError('LSM distinct group keys');
  for (const key of keys) if (typeof key !== 'string' || key.length === 0 || key.length > 128 || /[^A-Za-z0-9_.-]/.test(key)) throw TypeError('LSM key');
  return [...keys];
}
const token = Symbol();
export class PhysicalLsmJournalStore {
  #wal; #provider; #options; #state; #index = new Map(); #tables = new Map(); #leases = new Map();
  #recordCount = 0; #walBytes = 0; #closed = false; #fenced = false; #busy = false; #durable; #tableDurable;
  constructor(secret, wal, provider, options) {
    if (secret !== token) throw TypeError('use PhysicalLsmJournalStore.open');
    this.#wal=wal;this.#provider=provider;this.#options=options;
    this.#tableDurable=provider.supportsDurableFlush === true;
    this.#durable=wal.supportsDurableFlush === true && this.#tableDurable;
  }
  static open(wal,provider,selected={}) {
    const options=lsmOptions(selected);options.maxNamespaceEntries ??= 256;
    if (!Number.isSafeInteger(options.maxNamespaceEntries) || options.maxNamespaceEntries < 2 || options.maxNamespaceEntries > 65536) throw RangeError('LSM namespace bounds');
    if (wal?.supportsReplacement !== true || ['read','append','flush','replace'].some(key=>typeof wal[key] !== 'function')
      || ['list','create','open','close','delete'].some(key=>typeof provider?.[key] !== 'function')) throw TypeError('LSM requires a replaceable WAL and exclusive table namespace');
    const store=new PhysicalLsmJournalStore(token,wal,provider,options);
    try { store.#run(()=>store.#recover()); return store; }
    catch(cause) {
      const failures=store.#release();store.#closed=true;
      if(failures.length)throw new AggregateError([cause,...failures],'LSM recovery and lease cleanup failed');
      throw cause;
    }
  }
  get supportsDurableFlush() { return this.#durable; }
  #run(action) {
    if (this.#closed) throw Error('LSM store is closed');
    if (this.#fenced) throw Error('uncertain LSM owner is fenced; reopen');
    if (this.#busy) throw Error('LSM reentry is forbidden');
    this.#busy=true;
    try { const result=action();this.#fenced=false;return result; }
    finally { this.#busy=false; }
  }
  #effect(action) { this.#fenced=true;return sync(action()); }
  #tableOptions() { return { maxKeys:this.#options.maxKeys,maxBytes:67108864 }; }
  #checkDevice(device) {
    if(this.#tableDurable && device?.supportsDurableFlush!==true)throw TypeError('durable namespace returned a non-durable table device');
  }
  #record(row,owner) { return {key:row.key,generation:row.generation,tombstone:row.entries===null,cost:cost(row),owner}; }
  #recover() {
    const names=sync(this.#provider.list(this.#options.maxNamespaceEntries));
    if (!Array.isArray(names) || names.length>this.#options.maxNamespaceEntries)throw Error('incomplete LSM namespace');
    let last=0n;
    for(const id of names){const current=counter(id,true);if(current<=last)throw Error('unordered LSM namespace');last=current;}
    const text=sync(this.#wal.read());
    if(typeof text!=='string')throw TypeError('LSM WAL read must return a JSON record array');
    const records=JSON.parse(text);
    if(!Array.isArray(records))throw TypeError('LSM WAL record array');
    if(records.length===0){
      if(names.length)throw Error('uninitialized LSM namespace contains tables');
      this.#state={generation:'0',coveredGeneration:'0',lastReservedTableId:'0',manifest:[],buffer:[]};
      const initial=LsmWal.encode(this.#state,this.#options);this.#replace(initial);return;
    }
    this.#state=LsmWal.recover(records,this.#options);this.#recordCount=records.length;this.#walBytes=records.reduce((n,r)=>n+bytes(r),0);
    const selected=new Set(this.#state.manifest.map(item=>item.id));
    if(names.some(id=>BigInt(id)>BigInt(this.#state.lastReservedTableId)) || [...selected].some(id=>!names.includes(id)))throw Error('missing selected or unreserved LSM table');
    const latest=new Map(),seen=new Map();
    for(const descriptor of this.#state.manifest){
      const device=sync(this.#provider.open(descriptor.id));this.#leases.set(descriptor.id,device);
      this.#checkDevice(device);
      const table=SortedJournalTable.open(device,this.#tableOptions());this.#tables.set(descriptor.id,table);
      if(identity(table)!==descriptor.identity || table.digest!==descriptor.digest || table.count!==descriptor.count || table.storedBytes!==descriptor.storedBytes)throw Error('LSM selected table binding changed');
      for(const row of table.entries()){
        if(BigInt(row.generation)>BigInt(this.#state.coveredGeneration))throw Error('LSM table generation exceeds coverage');
        let versions=seen.get(row.key);
        if(versions===undefined){versions=new Map();seen.set(row.key,versions);}
        const previousOwner=versions.get(row.generation);
        if(previousOwner!==undefined){
          const previous=this.#tables.get(previousOwner).get(row.key);
          if(previous?.generation!==row.generation || !equal(previous.entries,row.entries))throw Error('conflicting LSM table versions');
        } else versions.set(row.generation,descriptor.id);
        const before=latest.get(row.key);
        if(before===undefined || BigInt(row.generation)>BigInt(before.generation)){
          latest.set(row.key,row);this.#index.set(row.key,this.#record(row,descriptor.id));
        }
      }
    }
    for(const row of this.#state.buffer){latest.set(row.key,row);this.#index.set(row.key,this.#record(row,'buffer'));}
    this.#admit(this.#index);
    const generation=[...latest.values()].reduce((n,row)=>BigInt(row.generation)>n?BigInt(row.generation):n,0n);
    if(generation!==BigInt(this.#state.generation))throw Error('LSM selected data is missing the global generation');
    for(const id of names)if(!selected.has(id))done(this.#effect(()=>this.#provider.delete(id)));
  }
  #admit(index) {
    if(index.size>this.#options.maxKeys || [...index.values()].reduce((n,row)=>n+row.cost,0)>this.#options.maxBytes)throw RangeError('LSM retained capacity exceeded');
  }
  #value(key) {
    const item=this.#index.get(key);if(item===undefined)return undefined;
    if(item.owner==='buffer')return this.#state.buffer.find(row=>row.key===key);
    const row=this.#tables.get(item.owner).get(key);
    if(row===undefined || row.generation!==item.generation || (row.entries===null)!==item.tombstone || cost(row)!==item.cost)throw Error('LSM point lookup changed');
    return row;
  }
  #version(key) { const row=this.#value(key);return {generation:row?.generation??'0',entries:row?.entries?.slice()??[]}; }
  read(keys) { return this.#run(()=>keysOf(keys).map(key=>this.#version(key))); }
  compareExchange(keys,expected,updates) {
    return this.#run(()=>{
      keys=keysOf(keys);
      if(!Array.isArray(expected)||!Array.isArray(updates)||expected.length!==keys.length||updates.length!==keys.length)throw TypeError('LSM group array lengths');
      const guards=Array.from(expected,item=>{counter(item?.generation);return item.generation;});
      const owned=Array.from(updates,items=>{
        if(items===null)return null;
        if(!Array.isArray(items))throw TypeError('LSM journal array');
        return Array.from(items,item=>{if(typeof item!=='string'||!item.isWellFormed())throw TypeError('LSM Unicode record');return item;});
      });
      if(keys.some((key,i)=>(this.#index.get(key)?.generation??'0')!==guards[i]))return false;
      const changes=[];
      for(let i=0;i<keys.length;i++)if(owned[i]!==null){
        const current=this.#value(keys[i]);
        if(current===undefined||!equal(current.entries,owned[i]))changes.push({key:keys[i],entries:owned[i]});
      }
      if(changes.length===0)return true;
      this.#commit(changes);return true;
    });
  }
  compareDelete(key,expectedGeneration) {
    return this.#run(()=>{
      keysOf([key]);counter(expectedGeneration,true);const current=this.#index.get(key);
      if(current===undefined||current.tombstone||current.generation!==expectedGeneration)return false;
      this.#commit([{key,entries:null}]);return true;
    });
  }
  #commit(changes) {
    if(this.#state.generation===maximum.toString())throw RangeError('LSM generation exhausted');
    const generation=(BigInt(this.#state.generation)+1n).toString();
    changes=ordered(changes.map(row=>({...row,generation})));
    const candidate=new Map(this.#index);
    for(const row of changes)candidate.set(row.key,this.#record(row,'buffer'));
    this.#admit(candidate);
    const changeBytes=changes.reduce((n,row)=>n+cost(row),0);
    if(changes.length>this.#options.bufferKeys||changeBytes>this.#options.bufferBytes)throw RangeError('LSM group exceeds buffer admission');
    const record=JSON.stringify(['put',generation,wire(changes)]), recordBytes=bytes(record);
    if(this.#options.maxWalRecords<2)throw RangeError('LSM writes require a checkpoint and group');
    let merged=new Map(this.#state.buffer.map(row=>[row.key,row]));for(const row of changes)merged.set(row.key,row);
    const plan=merged.size>this.#options.bufferKeys||[...merged.values()].reduce((n,row)=>n+cost(row),0)>this.#options.bufferBytes?this.#preparePublish(false):undefined;
    const base=plan?.published??this.#state;
    if(plan!==undefined)merged=new Map(changes.map(row=>[row.key,row]));
    // Include the reserved/new table descriptor before any maintenance I/O.
    if(bytes(LsmWal.encode(base,this.#options))+recordBytes>this.#options.maxWalBytes)throw RangeError('LSM checkpoint and following group exceed WAL admission');
    LsmWal.encode({...base,generation,buffer:ordered(merged.values())},this.#options);
    if(plan!==undefined)this.#publishPrepared(plan);
    const after={...this.#state,generation,buffer:ordered(merged.values())};
    if(this.#recordCount+1>this.#options.maxWalRecords||this.#walBytes+recordBytes>this.#options.maxWalBytes)this.#replace(LsmWal.encode(this.#state,this.#options));
    done(this.#effect(()=>this.#wal.append(record)));done(this.#effect(()=>this.#wal.flush()));
    this.#state=after;this.#recordCount++;this.#walBytes+=recordBytes;
    for(const row of changes)this.#index.set(row.key,this.#record(row,'buffer'));
  }
  #replace(record) {
    const result=this.#effect(()=>this.#wal.replace([record]));
    if(typeof result!=='boolean')throw TypeError('LSM WAL replacement result');
    this.#recordCount=1;this.#walBytes=bytes(record);
  }
  #preparePublish(compact) {
    if(this.#state.buffer.length===0&&(!compact||this.#state.manifest.length<=1))return;
    if(this.#state.lastReservedTableId===maximum.toString())throw RangeError('LSM table identity exhausted');
    const merge=compact||this.#state.manifest.length>=this.#options.maxRuns;
    const rows=merge?ordered([...this.#index.keys()].map(key=>this.#value(key))):this.#state.buffer;
    const storedBytes=15+rows.reduce((n,row)=>n+bytes(JSON.stringify([row.key,row.generation,row.entries])),0);
    if(storedBytes>67108864||this.#state.manifest.length+1>this.#options.maxNamespaceEntries)throw RangeError('LSM table/namespace admission');
    const id=(BigInt(this.#state.lastReservedTableId)+1n).toString();
    const reserved={...this.#state,lastReservedTableId:id};
    const reservation=LsmWal.encode(reserved,this.#options), retained=merge?[]:this.#state.manifest;
    const stub={id,identity:'0'.repeat(32),digest:maximum.toString(),count:rows.length,storedBytes};
    const published={...reserved,coveredGeneration:reserved.generation,manifest:[...retained,stub],buffer:[]};
    LsmWal.encode(published,this.#options);
    return {merge,rows,storedBytes,id,reserved,reservation,published};
  }
  #publishPrepared({merge,rows,storedBytes,id,reserved,reservation,published}) {
    // Reservation survives an uncertain create, so the name cannot be reused.
    this.#replace(reservation);this.#state=reserved;
    const device=this.#effect(()=>this.#provider.create(id));this.#leases.set(id,device);
    this.#checkDevice(device);
    const table=this.#effect(()=>SortedJournalTable.create(device,rows,this.#tableOptions()));
    this.#tables.set(id,table);
    const descriptor={id,identity:identity(table),digest:table.digest,count:table.count,storedBytes:table.storedBytes};
    if(descriptor.count!==rows.length||descriptor.storedBytes!==storedBytes)throw Error('LSM prepared table bounds changed');
    published.manifest=[...(merge?[]:reserved.manifest),descriptor];this.#replace(LsmWal.encode(published,this.#options));
    const obsolete=merge?this.#state.manifest:[];this.#state=published;
    for(const row of rows)this.#index.set(row.key,this.#record(row,id));
    for(const old of obsolete){
      done(this.#effect(()=>this.#provider.close(this.#leases.get(old.id))));this.#leases.delete(old.id);this.#tables.delete(old.id);
      done(this.#effect(()=>this.#provider.delete(old.id)));
    }
  }
  #publish(compact) { const plan=this.#preparePublish(compact);if(plan!==undefined)this.#publishPrepared(plan); }
  flush() { return this.#run(()=>this.#publish(false)); }
  compact() { return this.#run(()=>this.#publish(true)); }
  scanNamespace(selected) {
    return this.#run(()=>{
      const options=scanOptions(selected),result=[];let used=0;
      for(const item of ordered(this.#index.values()))if(!item.tombstone){
        const row=options.keysOnly?{generation:item.generation,entries:[]}:this.#version(item.key);
        used+=item.key.length+8+row.entries.reduce((n,entry)=>n+bytes(entry)+1,0);
        if(result.length===options.maxKeys||used>options.maxBytes)throw RangeError('LSM namespace scan admission');
        result.push({key:item.key,version:row});
      }
      return result;
    });
  }
  #release() {
    const errors=[];
    for(const [id,device] of [...this.#leases].reverse())try{done(sync(this.#provider.close(device)));this.#leases.delete(id);}catch(error){errors.push(error);}
    return errors;
  }
  close() {
    if(this.#closed)return;if(this.#busy)throw Error('LSM reentry is forbidden');this.#closed=true;
    const errors=this.#release();if(errors.length)throw new AggregateError(errors,'LSM cleanup failed; ownership remains fenced');
  }
  [Symbol.dispose]() { this.close(); }
}
