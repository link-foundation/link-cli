// The JavaScript clink against the Rust clink on transactions and version
// control: the scenarios of rust/tests/cli_transactions_and_vc_tests.rs, and
// the option edge cases of rust/src/main.rs (`parse_commit_mode`,
// `parse_retention`, `run_with_transactions`, `run_with_vc`), compared byte
// for byte. Log timestamps and transaction ids come from the clock, so they
// are masked, in the output and in the journals.
// Skipped without CLINK_RUST or without bin/clink.js.
import { existsSync } from 'node:fs';
import { describe, it } from 'node:test';
import {
  CLOCK_FILES,
  differential,
  JOURNAL_FILES,
  JS,
  maskClock,
  skip as noRust,
} from './harness.mjs';

const skip = noRust || (existsSync(JS) ? false : `${JS} is missing`);

/** `run_query`: `clink --db data.links <extra…> <query>`. */
const query = (text, ...extra) => ({ args: ['--db', 'data.links', ...extra, text] });

const groups = {
  'cli_transactions_and_vc_tests.rs': [
    [
      '--transactions creates the transitions sidecar',
      [query('() ((1 1) (2 2))', '--transactions')],
    ],
    [
      '--log prints the recorded transitions',
      [query('() ((1 1) (2 2))', '--transactions'), query('', '--transactions', '--log')],
    ],
    [
      '--transactions-file is honored',
      [query('() ((1 1))', '--transactions-file', 'custom-log.links')],
    ],
    ['--vc creates the version-control sidecar', [query('() ((1 1))', '--vc')]],
    [
      '--list-branches shows the default branch',
      [query('() ((1 1))', '--vc'), query('', '--vc', '--list-branches')],
    ],
    [
      '--branch creates a new branch',
      [
        query('() ((1 1))', '--vc'),
        query('() ((2 2))', '--vc', '--branch', 'feature'),
        query('', '--vc', '--list-branches'),
      ],
    ],
    [
      '--tag then --list-tags',
      [
        query('() ((1 1))', '--vc'),
        query('', '--vc', '--tag', 'v1'),
        query('', '--vc', '--list-tags'),
      ],
    ],
    ['an invalid --commit-mode is rejected', [query('() ((1 1))', '--commit-mode', 'bogus')]],
    ['an invalid --retention is rejected', [query('() ((1 1))', '--retention', 'bogus')]],
    ['no flags create no sidecars', [query('() ((1 1))')]],
  ],
  'option values': [
    [
      'commit modes in any case, blank values and retention specs',
      [
        query('() ((1 1))', '--commit-mode', ' ASYNC '),
        query('() ((2 2))', '--commit-mode', 'Sync'),
        query('() ((3 3))', '--commit-mode', '  '),
        query('() ((4 4))', '--retention', ' '),
        query('() ((5 5))', '--retention', 'infinite'),
        query('() ((6 6))', '--retention', 'sized:3'),
        query('((1: 1 1)) ()', '--retention', 'sized:3'),
        query('', '--log'),
      ],
    ],
    [
      'invalid retention specs',
      [
        query('() ((1 1))', '--retention', 'sized:'),
        query('() ((1 1))', '--retention', 'sized:0'),
        query('() ((1 1))', '--retention', 'sized:x'),
        query('() ((1 1))', '--retention', 'chunked:2'),
        query('() ((1 1))', '--retention', 'chunked:0:archive'),
        query('() ((1 1))', '--commit-mode', 'eventually'),
      ],
    ],
  ],
  'version control': [
    [
      'tags by sequence, by alias and at unknown points',
      [
        query('() ((1 1))', '--vc'),
        query('() ((2 2))', '--vc'),
        query('', '--vc', '--tag', 'release=1'),
        query('', '--vc', '--tag', 'alias=release=1'),
        query('', '--vc', '--tag', 'copy=release'),
        query('', '--vc', '--tag', 'lost=nowhere'),
        query('', '--vc', '--tag', 'blank='),
        query('', '--vc', '--list-tags'),
      ],
    ],
    [
      'checkout points, the log and branch listings',
      [
        query('() ((1 1))', '--vc'),
        query('() ((2 2))', '--vc'),
        query('', '--vc', '--tag', 'one=1'),
        query('', '--checkout', 'one', '--after'),
        query('', '--checkout', ' 2 ', '--after'),
        query('', '--checkout', ' '),
        query('', '--checkout', 'missing'),
        query('', '--vc', '--log'),
        query('', '--branch', 'side', '--branch-from', '1', '--list-branches'),
        query('() ((3 3))', '--branch', 'side', '--list-tags'),
        query('', '--branch', 'main', '--after'),
        query('', '--vc-file', 'branches.links', '--list-branches'),
      ],
    ],
  ],
};

// JOURNAL_FILES and the `--transactions-file custom-log.links` journal.
const JOURNALS = new RegExp(`${JOURNAL_FILES.source}|(^|/)custom-log\\.links$`);
const options = { mask: maskClock, opaqueFiles: CLOCK_FILES, journalFiles: JOURNALS };
for (const [group, cases] of Object.entries(groups)) {
  describe(`differential: transactions, ${group}`, () => {
    for (const [name, steps] of cases) {
      it(name, { skip }, () => differential(steps, options));
    }
  });
}

// Chunked retention archives under names made of the clock and a transaction
// id, so only the outputs are compared.
describe('differential: transactions, chunked retention', () => {
  it('archives full chunks and keeps the rest in the log', { skip }, () =>
    differential(
      [
        query('() ((1 1) (2 2) (3 3))', '--retention', 'chunked:2:archive'),
        query('((1: 1 1)) ()', '--retention', 'chunked:2:archive/'),
        query('', '--log'),
      ],
      { mask: maskClock, compareFiles: false },
    ),
  );
});
