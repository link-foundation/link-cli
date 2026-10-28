// Native ownership recipe for the separately selected two-copy page device.
import { FilePageDevice } from './engine-file-paged-log.js';
import { PagedCommitLog } from './engine-paged-log.js';
import { MirroredPageDevice } from './engine-mirrored-pages.js';

function closeAll(devices) {
  const errors = [];
  for (const device of devices.reverse()) { try { device.close(); } catch (error) { errors.push(error); } }
  if (errors.length) throw new AggregateError(errors, 'native mirror cleanup failed; uncertain owners remain fenced');
}
export class FileMirroredPagedCommitLog extends PagedCommitLog {
  #devices;
  #closed = false;
  constructor(path, options) {
    const devices = [];
    try {
      devices.push(new FilePageDevice(path)); devices.push(new FilePageDevice(path + '.mirror'));
      super(new MirroredPageDevice(...devices), options);
    } catch (error) {
      try { closeAll(devices); } catch (cleanup) { throw new AggregateError([error, cleanup], 'native mirror initialization and cleanup failed'); }
      throw error;
    }
    this.#devices = devices;
  }
  #check() { if (this.#closed) throw new Error('native mirror log is closed'); }
  read() { this.#check(); return super.read(); }
  append(record) { this.#check(); super.append(record); }
  flush() { this.#check(); super.flush(); }
  close() { if (!this.#closed) { this.#closed = true; closeAll(this.#devices); } }
  [Symbol.dispose]() { this.close(); }
}
