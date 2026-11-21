// Regenerates the golden unit store fixtures of js/server/test/fixtures from
// the Rust example rust/examples/unit_store_trace.rs:
//
//   cargo build --manifest-path rust/Cargo.toml --example unit_store_trace
//   node experiments/issue-111/unit-store-fixtures.mjs rust/target/debug/examples/unit_store_trace
//
// Every scenario is replayed by the Rust example on a fresh database file
// (or on the `initial` bytes); its output, the length of the file it leaves
// and the file itself without its trailing zeros are what the JavaScript
// port is checked against (js/server/test/unit-store-golden.test.mjs).
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { randomScript } from '../../js/server/test/helpers/unit-store-trace.mjs';

const FIXTURES = fileURLToPath(new URL('../../js/server/test/fixtures/', import.meta.url));

/** A basic script: free-list reuse, missing links, a reservation that fails. */
const BASIC = `create 0 0
create 1 1
point
create 1 2
create 2 1
create 1 2      # a duplicate pair
update 2 3 1
delete 3
delete 99       # missing
create 5 5      # reuses 3
ensure 10
get-or-create 1 2
get-or-create 7 7
delete 10
delete 9        # frees 9 and the free 8 under it
ensure 2        # exists
ensure 0        # cannot be reserved
update 4 0 0
delete 1
update 1 1 1    # missing
create 6 6
`;

/** The first 64 bytes zero, then 36 of 0xff: a partial item past the header. */
const PARTIAL = Buffer.concat([Buffer.alloc(64), Buffer.alloc(36, 0xff)]).toString('hex');

export const SCENARIOS = [
  { name: 'u64-basic', flags: [], script: BASIC },
  { name: 'u32-basic', flags: ['--u32'], script: BASIC },
  { name: 'u64-resolved', flags: ['--resolved'], script: BASIC },
  { name: 'u32-resolved', flags: ['--u32', '--resolved'], script: BASIC },
  { name: 'u64-random', flags: [], script: randomScript(7, 60, { unique: true }) },
  { name: 'u32-resolved-random', flags: ['--u32', '--resolved'], script: randomScript(8, 80) },
  {
    name: 'u64-partial',
    flags: [],
    script: 'create 1 1\ncreate 1 2\npoint\ndelete 1\n',
    initial: PARTIAL,
  },
];

/** `bytes` without its trailing zeros. */
export function trimZeros(bytes) {
  let end = bytes.length;
  while (end > 0 && bytes[end - 1] === 0) end--;
  return bytes.subarray(0, end);
}

/** Runs one scenario through the Rust example `binary`. */
export function runRust(binary, { flags, script, initial }) {
  const directory = mkdtempSync(join(tmpdir(), 'unit-store-rust-'));
  try {
    const database = join(directory, 'db.links');
    const scriptPath = join(directory, 'script.txt');
    writeFileSync(scriptPath, script);
    if (initial !== undefined) writeFileSync(database, Buffer.from(initial, 'hex'));
    const result = spawnSync(binary, [database, scriptPath, ...flags], { encoding: 'utf8' });
    return {
      status: result.status,
      stderr: result.stderr,
      output: result.stdout.split('\n').filter((line) => line !== ''),
      bytes: readFileSync(database),
    };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const binary = process.argv[2] ?? process.env.CLINK_RUST_UNIT_STORE;
  const scenarios = SCENARIOS.map((scenario) => {
    const { status, stderr, output, bytes } = runRust(binary, scenario);
    if (status !== 0) throw new Error(`${scenario.name}: ${stderr}`);
    writeFileSync(join(FIXTURES, `unit-store-${scenario.name}.links`), trimZeros(bytes));
    return { ...scenario, output, length: bytes.length };
  });
  writeFileSync(
    join(FIXTURES, 'unit-store-scenarios.json'),
    `${JSON.stringify(scenarios, null, 2)}\n`,
  );
  for (const { name, length, output } of scenarios) {
    console.log(`${name}: ${output.length} lines, ${length} bytes`);
  }
}
