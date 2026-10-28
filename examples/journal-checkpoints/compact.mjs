// Exercise real two-bank codecs through actual WASM on finite volatile images.
// These cloned images do not simulate power loss or provide durable storage.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { DeltaCommitLog, WasmEngine } from '../../js/client/src/index.js';
import { CheckpointPagedCommitLog } from '../../js/client/src/engine-checkpoint-pages.js';
import { MirroredPageDevice } from '../../js/client/src/engine-mirrored-pages.js';

class MemoryPages {
  supportsDurableFlush = false;
  constructor(image = { metadata: [undefined, undefined], pages: [] }) { this.image = structuredClone(image); }
  newDatabaseId() { return Uint8Array.from({ length: 16 }, (_, i) => i); }
  get dataLength() { return this.image.pages.length * 4096; }
  readMetadata(slot) { return this.image.metadata[slot]?.slice(); }
  writeMetadata(slot, bytes) { this.image.metadata[slot] = bytes.slice(); }
  syncMetadata() {}
  readPage(index) { return this.image.pages[index].slice(); }
  writePage(index, bytes) { assert.equal(index, this.image.pages.length); this.image.pages.push(bytes.slice()); }
  syncData() {}
  truncateData(length) { this.image.pages.length = length / 4096; }
  repairPage(index, bytes) { this.image.pages[index] = bytes.slice(); }
  completeRecovery() {}
}
const directory = resolve(process.env.CLINK_WASM ?? 'js/pkg');
const binding = await import(pathToFileURL(join(directory, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(directory, 'clink_wasm_bg.wasm')) });
const open = (mirrored, images) => {
  const devices = Array.from({ length: mirrored ? 5 : 3 }, (_, i) => new MemoryPages(images?.[i]));
  const banks = mirrored ? [new MirroredPageDevice(devices[1], devices[2]), new MirroredPageDevice(devices[3], devices[4])] : devices.slice(1);
  return { devices, log: new CheckpointPagedCommitLog(devices[0], banks, { maxPages: 64 }) };
};
const images = (devices) => devices.map(device => structuredClone(device.image));
const results = [];
for (const stateStructure of ['flat', 'shared', 'sorted-runs']) {
  for (const revisionEncoding of ['snapshot', 'changes']) {
    for (const history of [null, 2, 'all']) {
      for (const delta of [false, true]) {
        for (const mirrored of [false, true]) {
          const options = { stateStructure, revisionEncoding, history, isolation: 'snapshot' };
          const { log, devices } = open(mirrored);
          const decorate = (raw) => delta ? new DeltaCommitLog(raw) : raw;
          using engine = new WasmEngine(binding, options, decorate(log));
          assert.equal(engine.compactJournal(), true); assert.equal(engine.compactJournal(), false);
          for (let revision = 1; revision <= 6; revision++) {
            await using tx = engine.begin();
            if (revision === 1) {
              for (let id = 1n; id <= 7n; id++) assert.equal((await tx.create(0, 0)).index, id);
              await tx.delete(7);
            }
            await tx.setName(1, `雪 ${'x'.repeat(256)} ${revision}`); await tx.commit();
          }
          const pin = engine.snapshot();
          await using pending = engine.begin(); assert.equal((await pending.create(0, 0)).index, 8n);
          const expected = engine.exportCheckpoint();
          const changed = JSON.stringify(engine.journal()) !== JSON.stringify(expected);
          assert.equal(engine.compactJournal(), changed); assert.equal(engine.compactJournal(), false);
          assert.deepEqual(engine.journal(), expected);
          for (let revision = 0; revision <= 6; revision++) {
            if (history === 'all' || revision === 6 || (history === 2 && revision === 5))
              assert.equal(engine.snapshot(revision).revision, BigInt(revision));
            else assert.throws(() => engine.snapshot(revision), /not retained/);
          }
          const copy = open(mirrored, images(devices));
          using reopened = new WasmEngine(binding, options, decorate(copy.log));
          assert.deepEqual(reopened.snapshot(), engine.snapshot());
          await using check = reopened.begin(); assert.equal((await check.create(0, 0)).index, 8n); await check.rollback();
          await pending.commit();
          const first = engine.begin(), second = engine.begin();
          assert.equal((await first.create(1, 0)).index, 9n); assert.equal((await second.create(0, 1)).index, 10n);
          assert.deepEqual(await engine.commitGroup([first, second]), [{ status: 'committed', revision: 8n }, { status: 'committed', revision: 9n }]);
          assert.equal(pin.rows.length, 6); assert.equal(pin.rows[0].name.endsWith(' 6'), true);
          const next = engine.exportCheckpoint();
          const finalChanged = JSON.stringify(engine.journal()) !== JSON.stringify(next);
          assert.equal(engine.compactJournal(), finalChanged);
          assert.equal(engine.compactJournal(), false);
          const closed = images(devices);
          if (mirrored) {
            const primary = closed[1 + 2 * Number(log.epoch % 2n)];
            primary.pages[0][100] ^= 1; primary.metadata[0][120] ^= 1;
          }
          const final = open(mirrored, closed);
          using restored = new WasmEngine(binding, options, decorate(final.log));
          assert.deepEqual(restored.snapshot(), engine.snapshot());
          await using following = restored.begin(); assert.equal((await following.create(0, 0)).index, 11n); assert.equal(await following.commit(), 10n);
          const followingChanged = JSON.stringify(restored.journal()) !== JSON.stringify(restored.exportCheckpoint());
          assert.equal(restored.compactJournal(), followingChanged);
          results.push({ stateStructure, revisionEncoding, history, delta, mirrored, epoch: log.epoch.toString(), records: next.length, revision: '10', next: '11' });
        }
      }
    }
  }
}
console.log(JSON.stringify({ cases: results.length, results }, null, 2));
