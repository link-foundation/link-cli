import test from 'node:test';
import assert from 'node:assert/strict';
import { CheckpointPagedCommitLog } from '../src/engine-checkpoint-pages.js';
import { MirroredCheckpointRoot } from '../src/engine-checkpoint-root.js';
import { blank, makeDevices, durable, records } from './support/checkpoint-pages.mjs';

test('mirrored root recovers acknowledged epoch after one copy damage', () => {
  const { devices } = makeDevices(Array.from({ length: 4 }, blank));
  let root = new MirroredCheckpointRoot(devices[0], devices[3], 0);
  let log = new CheckpointPagedCommitLog(root, devices.slice(1, 3), { maxPages: 4 });
  log.append('old'); log.flush(); log.replace(['acknowledged']);
  const images = durable(devices); images[0].metadata[1][120] ^= 1;
  const reopened = makeDevices(images).devices;
  root = new MirroredCheckpointRoot(reopened[0], reopened[3], 0);
  log = new CheckpointPagedCommitLog(root, reopened.slice(1, 3), { maxPages: 4 });
  assert.equal(log.epoch, 1n); assert.deepEqual(records(log), ['acknowledged']);
  assert.deepEqual(durable(reopened)[0].metadata, durable(reopened)[3].metadata);
});
