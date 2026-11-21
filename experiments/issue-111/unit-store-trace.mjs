// The JavaScript counterpart of rust/examples/unit_store_trace.rs: replays a
// script of link operations against a `DoubletsStorage` and prints the result
// of every operation, so the database file it leaves can be compared byte for
// byte with the one the Rust example writes.
//
//   node experiments/issue-111/unit-store-trace.mjs <database> <script> [--resolved] [--u32]
//
// The script format is described in js/server/test/helpers/unit-store-trace.mjs.
import { readFileSync } from 'node:fs';

import { trace } from '../../js/server/test/helpers/unit-store-trace.mjs';

const argumentsList = process.argv.slice(2);
const [database, scriptPath] = argumentsList.filter((argument) => !argument.startsWith('--'));
if (scriptPath === undefined) {
  console.error('usage: unit-store-trace <database> <script> [--resolved] [--u32]');
  process.exit(1);
}
const lines = trace(database, readFileSync(scriptPath, 'utf8'), {
  resolved: argumentsList.includes('--resolved'),
  u32: argumentsList.includes('--u32'),
});
for (const line of lines) console.log(line);
