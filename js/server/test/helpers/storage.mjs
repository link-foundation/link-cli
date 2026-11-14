// Shared fixtures of the storage tests: scratch directories and a stderr that
// keeps what is written to it.
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after } from 'node:test';

/** A fresh directory, removed when the test file ends. */
export function scratchDirectory() {
  const directory = mkdtempSync(join(tmpdir(), 'link-cli-storage-'));
  after(() => rmSync(directory, { recursive: true, force: true }));
  return directory;
}

/** A writable that collects its text in `text`. */
export function capture() {
  return {
    text: '',
    write(chunk) {
      this.text += chunk;
    },
  };
}

/** The `[TRACE]` lines written to a `capture()`. */
export const traceLines = (output) => output.text.split('\n').filter((line) => line !== '');
