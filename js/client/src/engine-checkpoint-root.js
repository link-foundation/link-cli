// Optional two-copy checkpoint roots. Devices remain exclusive and caller-owned.
import { same, finish, fail } from './engine-page-format.js';
import { decodeDecision } from './engine-checkpoint-page-format.js';

export class MirroredCheckpointRoot {
  #devices;
  #flags;
  #repairs = new Map();
  #fenced = false;
  constructor(primary, secondary, bankFlags) {
    if (
      !primary ||
      !secondary ||
      primary === secondary ||
      (bankFlags !== 0 && bankFlags !== 1) ||
      [primary, secondary].some((device) => (device.metadataFlags ?? 0) !== 0)
    )
      throw new TypeError(
        'checkpoint root mirroring requires distinct raw roots and bank mode 0 or 1',
      );
    this.#devices = [primary, secondary];
    this.#flags = 2 | bankFlags;
  }
  get metadataFlags() {
    return this.#flags;
  }
  get supportsDurableFlush() {
    return this.#devices.every((device) => device.supportsDurableFlush === true);
  }
  #check() {
    if (this.#fenced) fail('uncertain checkpoint root is fenced; reopen both devices');
  }
  #slot(slot) {
    this.#check();
    if (slot !== 0 && slot !== 1) throw new RangeError('checkpoint root metadata slot');
  }
  #mutate(action) {
    this.#check();
    try {
      action();
    } catch (error) {
      this.#fenced = true;
      throw error;
    }
  }
  #both(method, ...args) {
    this.#mutate(() => {
      for (const device of this.#devices) finish(device[method](...args));
    });
  }
  newDatabaseId() {
    this.#check();
    return this.#devices[0].newDatabaseId();
  }
  get dataLength() {
    this.#check();
    for (const device of this.#devices)
      if (device.dataLength !== 0) fail('checkpoint root data must be empty');
    return 0;
  }
  readMetadata(slot) {
    this.#slot(slot);
    let failed = false;
    const raw = this.#devices.map((device) => {
      try {
        return device.readMetadata(slot);
      } catch {
        failed = true;
        return new Uint8Array(1);
      }
    });
    const states = raw.map((bytes) => decodeDecision(bytes, slot, this.#flags)),
      [a, b] = states;
    if (
      a !== undefined &&
      b !== undefined &&
      (!same(a.identity, b.identity) ||
        (a.epoch > b.epoch ? a.epoch - b.epoch : b.epoch - a.epoch) > 2n)
    )
      fail('conflicting checkpoint root copies');
    if (a === undefined && b === undefined) {
      const absent = raw.map((bytes) => bytes === undefined || bytes.length === 0);
      // A torn primary next to a never written secondary is a decision
      // interrupted before its acknowledgement: report the invalid bytes like
      // one device so the other slot is used. Garbage in both copies needs a
      // second fault and is refused.
      if (!(absent[0] && absent[1]) && (failed || !absent[1]))
        fail('both checkpoint root copies are unreadable');
      this.#repairs.delete(slot);
      return absent[0] ? undefined : raw[0];
    }
    const source = a === undefined || (b !== undefined && b.epoch > a.epoch) ? 1 : 0;
    const bytes = raw[source].slice(),
      destinations = raw
        .map((copy, side) => (copy !== undefined && same(copy, bytes) ? -1 : side))
        .filter((side) => side >= 0);
    this.#repairs.set(slot, { bytes, source, destinations });
    return bytes.slice();
  }
  completeRecovery() {
    this.#mutate(() => {
      for (const [slot, { bytes, source, destinations }] of this.#repairs)
        if (destinations.length) {
          const current = this.#devices[source].readMetadata(slot);
          if (current === undefined || !same(current, bytes))
            fail('checkpoint root repair source changed');
        }
      for (const [slot, { bytes, destinations }] of this.#repairs)
        for (const side of destinations) {
          finish(this.#devices[side].writeMetadata(slot, bytes));
          finish(this.#devices[side].syncMetadata(slot));
        }
      this.#repairs.clear();
    });
  }
  writeMetadata(slot, bytes) {
    this.#slot(slot);
    if (decodeDecision(bytes, slot, this.#flags) === undefined)
      fail('invalid checkpoint root publication');
    // The secondary copy is written only after the primary copy is durable,
    // so power loss tears at most one copy of a decision.
    this.#mutate(() => {
      const [primary, secondary] = this.#devices;
      finish(primary.writeMetadata(slot, bytes));
      finish(primary.syncMetadata(slot));
      finish(secondary.writeMetadata(slot, bytes));
    });
    this.#repairs.delete(slot);
  }
  // Only the secondary copy can be unsynced: writes and repairs sync the
  // primary copy before anything else is written.
  syncMetadata(slot) {
    this.#slot(slot);
    this.#mutate(() => finish(this.#devices[1].syncMetadata(slot)));
  }
  syncData() {
    this.#both('syncData');
  }
  truncateData(length) {
    this.#check();
    if (length !== 0) fail('checkpoint root data must be empty');
    this.#both('truncateData', 0);
  }
  readPage() {
    this.#check();
    fail('checkpoint root has no data pages');
  }
  writePage() {
    this.#check();
    fail('checkpoint root has no data pages');
  }
}
