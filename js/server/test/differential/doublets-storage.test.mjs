// The JavaScript unit store against the Rust one: rust/examples/unit_store_trace.rs
// and test/helpers/unit-store-trace.mjs replay the same scripts of link
// operations on fresh database files, which must end up byte for byte equal,
// the printed results included. Set CLINK_RUST_UNIT_STORE to the Rust example
// (`cargo build --manifest-path rust/Cargo.toml --example unit_store_trace`,
// then rust/target/debug/examples/unit_store_trace); the tests are skipped
// without it.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, it } from 'node:test';

import { scratchDirectory } from '../helpers/storage.mjs';
import { randomScript, trace } from '../helpers/unit-store-trace.mjs';

const RUST = process.env.CLINK_RUST_UNIT_STORE || '';
export const skip =
  RUST && existsSync(RUST)
    ? false
    : 'set CLINK_RUST_UNIT_STORE to the Rust unit_store_trace example to run these tests';

const MODES = [
  { name: 'usize', flags: [] },
  { name: 'u32', flags: ['--u32'] },
  { name: 'usize resolved', flags: ['--resolved'] },
  { name: 'u32 resolved', flags: ['--u32', '--resolved'] },
];

/** Replays `script` through the Rust example and through `DoubletsStorage`. */
function compare(script, flags, initial) {
  const directory = scratchDirectory();
  const rustDatabase = join(directory, 'rust.links');
  const jsDatabase = join(directory, 'js.links');
  const scriptPath = join(directory, 'script.txt');
  writeFileSync(scriptPath, script);
  if (initial !== undefined) {
    writeFileSync(rustDatabase, initial);
    writeFileSync(jsDatabase, initial);
  }
  const rust = spawnSync(RUST, [rustDatabase, scriptPath, ...flags], { encoding: 'utf8' });
  assert.equal(rust.status, 0, rust.stderr);
  const lines = trace(jsDatabase, script, {
    resolved: flags.includes('--resolved'),
    u32: flags.includes('--u32'),
  });
  assert.deepEqual(
    lines,
    rust.stdout.split('\n').filter((line) => line !== ''),
  );
  const [expected, actual] = [readFileSync(rustDatabase), readFileSync(jsDatabase)];
  assert.equal(actual.length, expected.length);
  assert.ok(actual.equals(expected), 'the database files differ');
}

describe('the unit store against Rust', { skip }, () => {
  for (const { name, flags } of MODES) {
    const unique = !flags.includes('--resolved');
    for (const seed of [11, 12]) {
      it(`replays the random script ${seed} (${name})`, () => {
        compare(randomScript(seed, 1500, { unique }), flags);
      });
    }

    it(`grows the file by a reserve step (${name})`, () => {
      const u32 = flags.includes('--u32');
      const initial = Buffer.alloc(4096);
      if (u32) initial.writeUInt32LE(2 ** 20 + 5, 0);
      else initial.writeBigUInt64LE(BigInt(2 ** 20 + 5), 0);
      const script = [
        'create 1 1',
        'create 2 2',
        'point',
        'delete 1048582',
        'create 3 4',
        'ensure 1048590',
        'delete 1048588',
        'update 1048583 7 7',
        '',
      ].join('\n');
      compare(script, flags, initial);
    });
  }

  const FIXTURES = new URL('../fixtures/', import.meta.url);
  const scenarios = JSON.parse(
    readFileSync(new URL('unit-store-scenarios.json', FIXTURES), 'utf8'),
  );
  for (const { name, flags, script, initial } of scenarios) {
    it(`replays the golden scenario ${name}`, () => {
      compare(script, flags, initial === undefined ? undefined : Buffer.from(initial, 'hex'));
    });
  }
});
