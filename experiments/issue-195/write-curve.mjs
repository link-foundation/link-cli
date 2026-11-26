// Prints the median write time per 25-write bucket for a clink command,
// to tell steady growth from noise: node write-curve.mjs <writes> -- <clink...>
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { clinkRunner, median, queryFor } from '../../.github/scripts/transactions-benchmark.mjs';

const writes = Number(process.argv[2]);
const command = process.argv.slice(process.argv.indexOf('--') + 1);
const directory = mkdtempSync(join(tmpdir(), 'write-curve-'));
const run = clinkRunner(command, directory);
let bucket = [];
for (let index = 1; index <= writes; index++) {
  bucket.push(run(queryFor(index), index));
  if (index % 25 === 0) {
    console.log(`writes ${index - 24}..${index}: median ${median(bucket).toFixed(1)} ms`);
    bucket = [];
  }
}
rmSync(directory, { recursive: true, force: true });
