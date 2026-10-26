// Selected native ownership; Node imports stay out of portable/root entries.
import { FilePageDevice } from './engine-file-paged-log.js';
import { MirroredPageDevice } from './engine-mirrored-pages.js';
import { CheckpointPagedCommitLog } from './engine-checkpoint-pages.js';

function closeAll(devices) {
  const errors = [];
  for (const device of devices.slice().reverse()) {
    try { device.close(); } catch (error) { errors.push(error); }
  }
  return errors;
}
export class FileCheckpointPagedCommitLog extends CheckpointPagedCommitLog {
  #devices;
  #closed = false;
  constructor(path, { mirroredBanks = false, ...options } = {}) {
    if (typeof mirroredBanks !== 'boolean') throw new TypeError('mirroredBanks must be boolean');
    const devices = [];
    const acquire = (name) => { const device = new FilePageDevice(name); devices.push(device); return device; };
    try {
      const root = acquire(path);
      const bank = (index) => {
        const name = path + `.bank${index}`, primary = acquire(name);
        return mirroredBanks ? new MirroredPageDevice(primary, acquire(name + '.mirror')) : primary;
      };
      const banks = [bank(0), bank(1)];
      super(root, banks, options);
    } catch (cause) {
      const errors = closeAll(devices);
      if (errors.length) throw new AggregateError([cause, ...errors], 'native checkpoint initialization and cleanup failed');
      throw cause;
    }
    this.#devices = devices;
  }
  #check() { if (this.#closed) throw new Error('native checkpoint log is closed'); }
  get epoch() { this.#check(); return super.epoch; }
  read() { this.#check(); return super.read(); }
  append(record) { this.#check(); super.append(record); }
  flush() { this.#check(); super.flush(); }
  replace(records) { this.#check(); return super.replace(records); }
  close() {
    if (this.#closed) return;
    this.#closed = true;
    const errors = closeAll(this.#devices);
    if (errors.length) throw new AggregateError(errors, 'native checkpoint cleanup failed; uncertain owners remain fenced');
  }
  [Symbol.dispose]() { this.close(); }
}
