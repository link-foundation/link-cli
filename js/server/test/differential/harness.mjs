// Runs the same `clink` invocations through the Rust executable
// (CLINK_RUST) and through bin/clink.js of this package, each in a sandbox
// directory of its own, and compares everything observable: standard output,
// standard error, the exit status and every file the run leaves behind, byte
// for byte. Paths in the invocations are relative to the sandbox, so the
// messages that quote them are the same for both.
//
// CLINK_DIFFERENTIAL_DEBUG=1 prints every invocation and its results.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

export const RUST = process.env.CLINK_RUST || '';
export const JS = fileURLToPath(new URL('../../bin/clink.js', import.meta.url));
const DEBUG = /^(1|true|yes)$/i.test(process.env.CLINK_DIFFERENTIAL_DEBUG ?? '');

/** Skip reason when no Rust executable is configured. */
export const skip =
  RUST && existsSync(RUST)
    ? false
    : 'set CLINK_RUST to the Rust clink to run the differential tests';

function files(root) {
  const found = {};
  const walk = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) walk(path);
      else found[relative(root, path).split('\\').join('/')] = readFileSync(path);
    }
  };
  walk(root);
  return found;
}

function invoke(command, args, cwd, input) {
  const result = spawnSync(command, args, {
    cwd,
    input: input ?? '',
    encoding: 'buffer',
    timeout: 60_000,
    env: { ...process.env, RUST_BACKTRACE: '0', NO_COLOR: '1' },
  });
  if (result.error) throw result.error;
  return {
    status: result.status,
    signal: result.signal,
    stdout: result.stdout.toString('utf8'),
    stderr: result.stderr.toString('utf8'),
  };
}

/**
 * The transition log records the wall clock: its timestamp and the
 * transaction id (milliseconds in the high 64 bits) differ between any two
 * runs, so `--log` lines keep their shape with those two columns masked.
 */
export function maskClock(text) {
  return text.replace(/^(\d+)\t\d+\t(\w+)\t[0-9a-f]{32}\t/gm, '$1\t<time>\t$2\t<transaction>\t');
}

/**
 * The names sidecars of the transition log and of version control: they name
 * links by transaction id and time, so two Rust runs already write different
 * bytes. They are compared by name only; `--log`, `--list-tags` and
 * `--list-branches` check what they hold. `log.links` is the transition log
 * of `--transactions-file log.links`.
 */
export const CLOCK_FILES = /(^|\/)(log|[^/]*\.transitions|[^/]*\.versioncontrol)\.names\.links$/;

/**
 * Runs `steps` -- `{ args, input?, files? }`, where `files` are written into
 * the sandbox first -- through both executables and asserts that every step
 * and the final sandbox are identical. `mask` rewrites both outputs before
 * they are compared and `opaqueFiles` names files compared by name only.
 * Returns the Rust results.
 */
export function differential(
  steps,
  { compareFiles = true, mask = (text) => text, opaqueFiles = null } = {},
) {
  const sandboxes = {
    rust: mkdtempSync(join(tmpdir(), 'clink-rust-')),
    js: mkdtempSync(join(tmpdir(), 'clink-js-')),
  };
  try {
    const results = [];
    for (const [index, step] of steps.entries()) {
      for (const sandbox of Object.values(sandboxes)) {
        for (const [name, content] of Object.entries(step.files ?? {})) {
          mkdirSync(dirname(join(sandbox, name)), { recursive: true });
          writeFileSync(join(sandbox, name), content);
        }
      }
      const rust = invoke(RUST, step.args, sandboxes.rust, step.input);
      const js = invoke(process.execPath, [JS, ...step.args], sandboxes.js, step.input);
      if (DEBUG) {
        console.error(`step ${index}: clink ${JSON.stringify(step.args)}`);
        console.error(`  rust: ${JSON.stringify(rust)}`);
        console.error(`  js:   ${JSON.stringify(js)}`);
      }
      const where = `step ${index}: clink ${JSON.stringify(step.args)}`;
      assert.equal(mask(js.stderr), mask(rust.stderr), `${where}: standard error`);
      assert.equal(mask(js.stdout), mask(rust.stdout), `${where}: standard output`);
      assert.equal(js.status, rust.status, `${where}: exit status`);
      if (compareFiles) {
        const rustFiles = files(sandboxes.rust);
        const jsFiles = files(sandboxes.js);
        assert.deepEqual(
          Object.keys(jsFiles).sort(),
          Object.keys(rustFiles).sort(),
          `${where}: files`,
        );
        for (const [name, bytes] of Object.entries(rustFiles)) {
          if (opaqueFiles?.test(name)) continue;
          assert.ok(jsFiles[name].equals(bytes), `${where}: the bytes of ${name}`);
        }
      }
      results.push(rust);
    }
    return results;
  } finally {
    for (const sandbox of Object.values(sandboxes))
      rmSync(sandbox, { recursive: true, force: true });
  }
}
