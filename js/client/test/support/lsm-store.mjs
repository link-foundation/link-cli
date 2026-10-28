// Finite real checkpoint-WAL/table image harness; no process/power-loss claim.
import assert from 'node:assert/strict';
import { Device } from './sorted-table.mjs';
import { CheckpointPagedCommitLog } from '../../src/engine-checkpoint-pages.js';

class FaultDevice extends Device {
  constructor(control, name, image) { super(image); this.control = control; this.name = name; }
  action(name, body) {
    return this.control.effect(this.name + ':' + name, () => super.action(name, body), () => {
      // A complete unsynchronized decision is allowed to survive a lost reply.
      if (name.startsWith('writeMetadata')) this.durable.metadata = structuredClone(this.live.metadata);
    });
  }
}
export class LsmHarness {
  trace = []; reads = []; fault; leases = new Set();
  constructor(image) {
    this.devices = ['root', 'bank0', 'bank1'].map((name, i) => new FaultDevice(this, name, image?.wal[i]));
    this.tables = new Map((image?.tables ?? []).map(([id, bytes]) => [id, new FaultDevice(this, 'table-' + id, bytes)]));
    this.wal = new CheckpointPagedCommitLog(this.devices[0], this.devices.slice(1), { maxPages: 128 });
    const control = this;
    this.provider = {
      supportsDurableFlush: true,
      list(maxEntries) {
        control.reads.push('list');
        if (control.tables.size > maxEntries) throw Error('namespace admission');
        return [...control.tables.keys()].sort((a,b) => BigInt(a) < BigInt(b) ? -1 : 1);
      },
      create(id) {
        let device;
        try {
          return control.effect('create:' + id, () => {
            assert.equal(control.tables.has(id), false, 'never reuse a physical name');
            device = new FaultDevice(control, 'table-' + id);
            control.tables.set(id, device); control.leases.add(device); return device;
          });
        } catch (error) { control.leases.delete(device); throw error; }
      },
      open(id) {
        control.reads.push('open:' + id);
        const device = control.tables.get(id);
        if (device === undefined) throw Error('missing selected table');
        assert.equal(control.leases.has(device), false, 'exclusive table lease');
        control.leases.add(device); return device;
      },
      close(device) { return control.effect('close:' + device.name, () => { control.leases.delete(device); }); },
      delete(id) {
        assert.equal(control.leases.has(control.tables.get(id)), false, 'close before retirement');
        return control.effect('delete:' + id, () => { assert.equal(control.tables.delete(id), true); });
      },
    };
  }
  effect(name, action, persistUncertainDecision = () => {}) {
    this.trace.push(name);
    const fault = this.fault?.at === this.trace.length;
    if (fault && !this.fault.after) throw Error('injected ' + name);
    const result = action();
    if (fault) { persistUncertainDecision(); throw Error('injected ' + name); }
    return result;
  }
  image() { return { wal:this.devices.map(d => structuredClone(d.durable)), tables:[...this.tables].map(([id,d]) => [id,structuredClone(d.durable)]) }; }
  crash() { return new LsmHarness(this.image()); }
  reset() { this.trace = []; this.reads = []; for (const d of [...this.devices,...this.tables.values()]) d.calls=[]; }
}
export const selected = { maxKeys:32,maxBytes:4096,bufferKeys:2,bufferBytes:128,maxRuns:2,maxWalRecords:4,maxWalBytes:8192,maxNamespaceEntries:16 };
