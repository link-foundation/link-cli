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

/**
 * Why a test that needs POSIX permission bits is skipped: on Windows, `chmod`
 * only toggles the read-only attribute. `false` where the test runs.
 */
export const NO_POSIX_PERMISSIONS =
  process.platform === 'win32' && 'Windows has no POSIX permission bits (chmod)';

/** `text` as a regular expression that matches exactly it, such as a Windows path. */
export const literally = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

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
