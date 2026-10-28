import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync, readFileSync, existsSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { FilePagedCommitLog } from './engine-file-paged-log.mjs';
const adapter = new URL('./engine-file-paged-log.mjs', import.meta.url).href;

test('real native pages reopen Unicode groups and exclude another writer', () => {
  const dir = mkdtempSync(join(tmpdir(), 'clink-pages-')), path = join(dir, 'log');
  try {
    const expected = ['', '雪'.repeat(700), '\ufeff\0😀'];
    using first = new FilePagedCommitLog(path);
    assert.throws(() => new FilePagedCommitLog(path), /EEXIST/);
    for (const record of expected) first.append(record);
    first.flush(); first.close();
    using reopened = new FilePagedCommitLog(path);
    assert.deepEqual(JSON.parse(reopened.read()), expected);
    reopened.append('next'); reopened.flush(); reopened.close(); reopened.close();
    using third = new FilePagedCommitLog(path);
    assert.deepEqual(JSON.parse(third.read()), [...expected, 'next']);
    for (const action of [() => first.read(), () => first.append('x'), () => first.flush()]) assert.throws(action, /closed/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('failed open preserves existing files and releases only its own ownership', () => {
  const dir = mkdtempSync(join(tmpdir(), 'clink-pages-')), path = join(dir, 'log');
  try {
    writeFileSync(path, 'unrelated file');
    assert.throws(() => new FilePagedCommitLog(path), /metadata/);
    assert.equal(readFileSync(path, 'utf8'), 'unrelated file');
    assert.equal(existsSync(path + '.writer'), false);
    assert.throws(() => new FilePagedCommitLog(join(dir, 'missing', 'log')), /ENOENT/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('finite real process exits preserve acknowledged groups and require explicit owner fencing', () => {
  const dir = mkdtempSync(join(tmpdir(), 'clink-pages-'));
  try {
    for (const phase of ['append', 'flush']) {
      const path = join(dir, phase);
      using first = new FilePagedCommitLog(path);
      first.append('acknowledged'); first.flush(); first.close();
      const child = spawnSync(process.execPath, ['--max-old-space-size=96', '--input-type=module', '-e',
        `import {FilePagedCommitLog} from ${JSON.stringify(adapter)}; const log=new FilePagedCommitLog(process.argv[1]); log.append('pending 雪'); if(process.argv[2]==='flush')log.flush(); process.exit(73);`, path, phase],
        { encoding: 'utf8', timeout: 10_000, maxBuffer: 64 * 1024 });
      assert.equal(child.error, undefined); assert.equal(child.status, 73, child.stderr);
      assert.throws(() => new FilePagedCommitLog(path), /EEXIST/);
      // The child has exited; the old writer is now demonstrably fenced.
      unlinkSync(path + '.writer');
      using reopened = new FilePagedCommitLog(path);
      const expected = phase === 'flush' ? ['acknowledged', 'pending 雪'] : ['acknowledged'];
      assert.deepEqual(JSON.parse(reopened.read()), expected);
      reopened.append('following'); reopened.flush(); reopened.close();
      using again = new FilePagedCommitLog(path);
      assert.deepEqual(JSON.parse(again.read()), [...expected, 'following']);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
