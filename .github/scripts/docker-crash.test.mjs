import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import test from 'node:test';
import {
  SECTOR,
  checkAcks,
  checkSurvivors,
  losePower,
  openedRevision,
  parseOutput,
  random,
  recordAt,
  recordStarts,
  targets,
} from './docker-crash.mjs';

test('a seed replays the same choices', { timeout: 10000 }, () => {
  const first = random(7);
  const second = random(7);
  const values = Array.from({ length: 50 }, () => first());
  assert.deepEqual(
    Array.from({ length: 50 }, () => second()),
    values,
  );
  assert.ok(values.every((value) => value >= 0 && value < 1));
  assert.notDeepEqual(Array.from({ length: 50 }, random(8)), values);
  const below = random(1);
  for (let i = 0; i < 200; i++) {
    const value = below.below(5);
    assert.ok(Number.isInteger(value) && value >= 0 && value < 5);
  }
});

test('writer output separates acknowledgements from everything else', { timeout: 10000 }, () => {
  assert.deepEqual(parseOutput('ack 1\nack 2\n\nerror disk full\nack 3x\n'), {
    acks: [1, 2],
    other: ['error disk full', 'ack 3x'],
  });
  assert.deepEqual(parseOutput(''), { acks: [], other: [] });
});

test('acknowledgements must continue the verified revision one by one', { timeout: 10000 }, () => {
  assert.equal(checkAcks([], 5), 5);
  assert.equal(checkAcks([6, 7, 8], 5), 8);
  assert.throws(() => checkAcks([7], 5), /acknowledged 7 after revision 5/);
  assert.throws(() => checkAcks([6, 8], 5), /acknowledged 6,8/);
  assert.throws(() => checkAcks([5], 5), /after revision 5/);
});

test(
  'a crash may leave one unacknowledged commit, never lose an acknowledged one',
  { timeout: 10000 },
  () => {
    const at = (last, acknowledged, primary, replica) => ({ last, acknowledged, primary, replica });
    checkSurvivors(at(10, 10, 10, 10));
    checkSurvivors(at(10, 10, 11, 10));
    checkSurvivors(at(10, 10, 11, 11));
    // Killed before its first acknowledgement, a writer may leave the replica
    // behind an earlier unacknowledged commit, or add one more of its own.
    checkSurvivors(at(11, 10, 11, 10));
    checkSurvivors(at(11, 10, 12, 10));
    assert.throws(() => checkSurvivors(at(10, 10, 9, 9)), /primary at 9/);
    assert.throws(() => checkSurvivors(at(10, 10, 12, 10)), /primary at 12/);
    assert.throws(() => checkSurvivors(at(10, 10, 10, 9)), /replica at 9/);
    assert.throws(() => checkSurvivors(at(10, 10, 10, 11)), /replica at 11/);
    assert.throws(() => checkSurvivors(at(11, 11, 11, 10)), /replica at 10/);
    assert.throws(() => checkSurvivors(at(10, 11, 11, 11)), /beyond revision 10/);
  },
);

test('record boundaries locate the record a damaged byte belongs to', { timeout: 10000 }, () => {
  const bytes = Buffer.from('ab\ncde\nf\n');
  const starts = recordStarts(bytes);
  assert.deepEqual(starts, [0, 3, 7, 9]);
  assert.deepEqual(
    [0, 2, 3, 6, 7, 8].map((offset) => recordAt(starts, offset)),
    [0, 0, 1, 1, 2, 2],
  );
  // A torn tail without its newline still belongs to the record after the last one.
  const torn = recordStarts(Buffer.from('ab\ncd'));
  assert.deepEqual(torn, [0, 3]);
  assert.equal(recordAt(torn, 4), 1);
});

test(
  'power loss keeps every durable byte and only cuts or zeroes the rest',
  { timeout: 10000 },
  () => {
    const bytes = Buffer.alloc(5 * SECTOR + 100, 'x');
    const durable = SECTOR + 37;
    const next = random(11);
    const seen = { lengths: new Set(), zeroed: 0, kept: 0 };
    for (let trial = 0; trial < 300; trial++) {
      const { bytes: after, cut, zeroed } = losePower(bytes, durable, next);
      assert.equal(after.length, bytes.length - cut);
      assert.ok(after.length >= durable && after.length <= bytes.length);
      assert.ok(after.subarray(0, durable).equals(bytes.subarray(0, durable)));
      for (let offset = durable; offset < after.length; offset++) {
        const sector = Math.floor(offset / SECTOR);
        assert.equal(after[offset], zeroed.includes(sector) ? 0 : bytes[offset], `byte ${offset}`);
      }
      // The sector holding the bound is zeroed only past it.
      assert.ok(zeroed.every((sector) => sector >= 1 && sector * SECTOR < after.length));
      seen.lengths.add(
        after.length === durable ? 'none' : after.length === bytes.length ? 'all' : 'some',
      );
      seen.zeroed += zeroed.length;
      seen.kept += zeroed.length === 0 && after.length === bytes.length ? 1 : 0;
    }
    assert.deepEqual([...seen.lengths].sort(), ['all', 'none', 'some']);
    assert.ok(seen.zeroed > 0 && seen.kept > 0);
    // Nothing past the durable bound: nothing to lose.
    assert.deepEqual(losePower(bytes, bytes.length, random(1)).bytes, bytes);
    assert.throws(() => losePower(bytes, bytes.length + 1, random(1)), /beyond/);
  },
);

test(
  'a journal opens at its intact lines and refuses a hole before one',
  { timeout: 10000 },
  () => {
    const open = (text) => openedRevision(Buffer.from(text));
    assert.deepEqual(open(''), { revision: 0, end: 0, zeroedLines: 0 });
    assert.deepEqual(open('ab\ncd\n'), { revision: 2, end: 6, zeroedLines: 0 });
    assert.deepEqual(open('ab\ncd\nef'), { revision: 2, end: 6, zeroedLines: 0 });
    assert.deepEqual(open('ab\nc\0\n'), { revision: 1, end: 3, zeroedLines: 1 });
    assert.deepEqual(open('ab\nc\0\n\0\0\n\0'), { revision: 1, end: 3, zeroedLines: 2 });
    assert.deepEqual(open('\0b\nc\0\n'), { revision: 0, end: 0, zeroedLines: 2 });
    // A zeroed line in front of an intact one is refused; the intact one still
    // tells which record reached the disk.
    assert.deepEqual(open('ab\nc\0\nef\n'), { revision: null, end: 9, zeroedLines: 0 });
  },
);

test(
  'every target names a pinned image, a writer and its default artifact',
  { timeout: 10000 },
  () => {
    assert.deepEqual(Object.keys(targets).sort(), ['csharp', 'js', 'rust']);
    for (const [name, target] of Object.entries(targets)) {
      assert.match(target.image, /^[a-z0-9./-]+:[0-9a-z.-]+$/, name);
      assert.ok(target.command.length > 0, name);
      for (const [source, destination] of target.mounts(target.artifact)) {
        assert.ok(destination.startsWith('/app'), `${name} mounts ${destination}`);
        assert.equal(typeof source, 'string');
      }
    }
    // The JS writer is part of the repository; the other artifacts are built.
    assert.ok(
      existsSync(new URL('../../js/client/examples/engine-crash-writer.mjs', import.meta.url)),
    );
    assert.equal(targets.js.writerLocks, true);
  },
);
