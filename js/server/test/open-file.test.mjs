// `openFileSync` (src/open-file.js): a directory opened in place of a file
// fails as `open(2)` does on every platform. Linux refuses it in `open`
// already; a fake `node:fs` stands for libuv on Windows, which opens it.
import assert from 'node:assert/strict';
import { closeSync, mkdirSync, readSync, writeFileSync } from 'node:fs';
import { constants } from 'node:os';
import { join } from 'node:path';
import { describe, it } from 'node:test';

import { formatErrorReport } from '../src/error-report.js';
import { isDirectoryError, openFileSync } from '../src/open-file.js';
import { ioError } from '../src/transactions-storage.js';
import { scratchDirectory } from './helpers/storage.mjs';

const { errno } = constants;

/** A `node:fs` whose `fstatSync` answers `stat` (or throws it), recording closes. */
function fakeFs(stat) {
  const closed = [];
  return {
    closed,
    openSync: (path, flags, mode) => {
      assert.deepEqual([path, flags, mode], ['dir', 'r+', 0o666]);
      return 7;
    },
    fstatSync: (fd) => {
      assert.equal(fd, 7);
      if (stat instanceof Error) throw stat;
      return { isDirectory: () => stat === 'directory' };
    },
    closeSync: (fd) => closed.push(fd),
  };
}

describe('openFileSync', () => {
  it('opens a regular file and hands its descriptor back', () => {
    const path = join(scratchDirectory(), 'file.log');
    writeFileSync(path, 'text');
    const fd = openFileSync(path, 'r+');
    try {
      const bytes = Buffer.alloc(4);
      assert.equal(readSync(fd, bytes, 0, 4, 0), 4);
      assert.equal(bytes.toString(), 'text');
    } finally {
      closeSync(fd);
    }
    const fs = fakeFs('file');
    assert.equal(openFileSync('dir', 'r+', 0o666, fs), 7);
    assert.deepEqual(fs.closed, []);
  });

  it('closes a directory and fails as open(2) does', () => {
    const fs = fakeFs('directory');
    assert.throws(() => openFileSync('dir', 'r+', 0o666, fs), {
      code: 'EISDIR',
      errno: -errno.EISDIR,
      syscall: 'open',
      path: 'dir',
      message: "EISDIR: illegal operation on a directory, open 'dir'",
    });
    assert.deepEqual(fs.closed, [7]);
    assert.equal(
      formatErrorReport(ioError(isDirectoryError('dir'))),
      `I/O error: Is a directory (os error ${errno.EISDIR})\n\nCaused by:\n    Is a directory (os error ${errno.EISDIR})`,
    );
  });

  it('closes the descriptor when fstat fails', () => {
    const failure = Object.assign(new Error('EIO: i/o error, fstat'), { code: 'EIO' });
    const fs = fakeFs(failure);
    assert.throws(() => openFileSync('dir', 'r+', 0o666, fs), failure);
    assert.deepEqual(fs.closed, [7]);
  });

  it('refuses a real directory for writing', () => {
    const path = join(scratchDirectory(), 'directory');
    mkdirSync(path);
    assert.throws(() => openFileSync(path, 'r+'), { code: 'EISDIR' });
  });
});
