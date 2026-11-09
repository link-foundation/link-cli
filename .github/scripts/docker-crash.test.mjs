import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import test from 'node:test';
import {
  checkAcks,
  checkSurvivors,
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
