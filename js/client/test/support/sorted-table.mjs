// Finite synchronous image device. No production codec is imported.
export class Device {
  supportsDurableFlush = true; calls = []; fault; reads = 0;
  constructor(image = { metadata: [undefined,undefined],pages:[] }) { this.live=structuredClone(image);this.durable=structuredClone(image); }
  newDatabaseId(){this.reads++;return Uint8Array.from({length:16},(_,i)=>i);}
  get dataLength(){this.reads++;return this.live.pages.length*4096;}
  readMetadata(slot){this.reads++;return this.live.metadata[slot]?.slice();}
  readPage(index){this.calls.push('readPage');return this.live.pages[index]?.slice();}
  action(name,body){this.calls.push(name);const fault=this.fault?.at===this.calls.length;if(fault&&!this.fault.after)throw Error('injected '+name);body();if(fault){if(name.startsWith('writeMetadata'))this.durable.metadata=structuredClone(this.live.metadata);throw Error('injected '+name);}}
  writePage(index,bytes){this.action('writePage',()=>{this.live.pages[index]=bytes.slice();});}
  repairPage(index,bytes){this.action('repairPage',()=>{this.live.pages[index]=bytes.slice();});}
  syncData(){this.action('syncData',()=>{this.durable.pages=structuredClone(this.live.pages);});}
  writeMetadata(slot,bytes){this.action('writeMetadata'+slot,()=>{this.live.metadata[slot]=bytes.slice();});}
  syncMetadata(slot){this.action('syncMetadata'+slot,()=>{this.durable.metadata[slot]=this.live.metadata[slot]?.slice();});}
  truncateData(length){this.action('truncateData',()=>{this.live.pages.length=length/4096;});}
  crash(){return new Device(this.durable);}
}
