// Issue #195: every `clink --transactions` write re-read and rewrote the whole
// transitions store, so write N+100 took several times as long as write N.
// The benchmark CI runs against each `clink` must catch that again.
import assert from 'node:assert/strict';
import test from 'node:test';

import {
  benchmark,
  DEFAULTS,
  evaluate,
  main,
  median,
  parseArgs,
  queryFor,
} from './transactions-benchmark.mjs';

// A stand-in `clink`: from its sixth write on, every write sleeps 600 ms more,
// far beyond the start-up noise of a loaded machine.
const slowingClink = [
  process.execPath,
  '-e',
  `const fs = require('node:fs');
   const counter = process.argv[process.argv.indexOf('--db') + 1] + '.count';
   const n = fs.existsSync(counter) ? Number(fs.readFileSync(counter, 'utf8')) : 0;
   fs.writeFileSync(counter, String(n + 1));
   Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, n >= 5 ? 600 : 0);`,
  '--', // the arguments that follow are the script's, not node's
];

test('parseArgs reads options and the command after --', () => {
  assert.deepEqual(parseArgs(['--', 'clink']), { options: { ...DEFAULTS }, command: ['clink'] });
  const { options, command } = parseArgs([
    '--gap',
    '5',
    '--window',
    '3',
    '--max-ratio',
    '1.5',
    '--slack-ms',
    '0',
    '--warmup',
    '0',
    '--',
    'dotnet',
    'clink.dll',
  ]);
  assert.deepEqual(options, { warmup: 0, gap: 5, window: 3, maxRatio: 1.5, slackMs: 0 });
  assert.deepEqual(command, ['dotnet', 'clink.dll']);
  assert.throws(() => parseArgs(['clink']), /usage/);
  assert.throws(() => parseArgs(['--']), /usage/);
  assert.throws(() => parseArgs(['--gap', '--', 'clink']), /invalid option --gap/);
  assert.throws(() => parseArgs(['--gap', '-1', '--', 'clink']), /invalid option/);
  assert.throws(() => parseArgs(['--speed', '1', '--', 'clink']), /invalid option --speed 1/);
  assert.throws(() => parseArgs(['--window', '0', '--', 'clink']), /at least 1/);
  assert.throws(() => parseArgs(['--max-ratio', '0.5', '--', 'clink']), /at least 1/);
});

test('writes keep the store at two links', () => {
  assert.equal(queryFor(1), '() ((1: 1 1) (2: 2 2))');
  assert.equal(queryFor(2), '((1: 1 1)) ((1: 1 2))');
  assert.equal(queryFor(3), '((1: 1 2)) ((1: 1 1))');
});

test('median and evaluate compare the two windows', () => {
  assert.equal(median([3, 1, 2]), 2);
  assert.equal(median([4, 1, 3, 2]), 2.5);
  assert.throws(() => median([]), /no values/);
  assert.deepEqual(evaluate([10, 10], [25, 25], { maxRatio: 2, slackMs: 5 }), {
    before: 10,
    after: 25,
    limit: 25,
    ok: true,
  });
  assert.equal(evaluate([10], [26], { maxRatio: 2, slackMs: 5 }).ok, false);
});

test('benchmark times the window at write N and at write N + gap', () => {
  const seen = [];
  const result = benchmark(
    { warmup: 2, gap: 4, window: 2, maxRatio: 3, slackMs: 0 },
    (query, index) => {
      seen.push(index);
      return index;
    },
  );
  assert.deepEqual(seen, [1, 2, 3, 4, 5, 6, 7, 8]);
  assert.deepEqual(
    { first: result.first, second: result.second, last: result.last },
    { first: 3, second: 7, last: 8 },
  );
  assert.deepEqual(result.early, [3, 4]);
  assert.deepEqual(result.late, [7, 8]);
  assert.equal(result.ok, true);
});

test('main passes a steady clink and fails one that slows down', () => {
  const lines = [];
  const steady = [process.execPath, '-e', '', '--'];
  assert.equal(
    main(
      ['--warmup', '0', '--gap', '2', '--window', '2', '--slack-ms', '1000', '--', ...steady],
      (line) => lines.push(line),
    ),
    0,
  );
  assert.match(
    lines.join('\n'),
    /writes 1\.\.2: median .* ms\nwrites 3\.\.4: median .* ms \(limit .* ms\)\nfiles after 4 writes: /,
  );

  lines.length = 0;
  const options = [
    '--warmup',
    '0',
    '--gap',
    '5',
    '--window',
    '3',
    '--max-ratio',
    '1',
    '--slack-ms',
    '0',
  ];
  assert.equal(
    main([...options, '--', ...slowingClink], (line) => lines.push(line)),
    1,
  );
  assert.match(
    lines.at(-1),
    /::error::clink --transactions slowed down: write 6 is not within 1x \+ 0 ms of write 1/,
  );
});

test('main reports a failing write', () => {
  const failing = [process.execPath, '-e', 'process.stderr.write("boom"); process.exit(3)', '--'];
  assert.throws(() => main(['--', ...failing], () => {}), /write 1 failed \(exit 3\): boom/);
  assert.throws(() => main(['--', '/nonexistent/clink'], () => {}), /ENOENT/);
});
