// The JavaScript clink against the Rust clink: the same invocations, the same
// standard output, standard error, exit status and files. Set CLINK_RUST to
// the Rust executable (`cargo build --release --bin clink`); the tests are
// skipped without it. CLINK_DIFFERENTIAL_DEBUG=1 prints every run.
import { describe, it } from 'node:test';
import { CLOCK_FILES, differential, maskClock, skip } from './harness.mjs';

const run = (...args) => ({ args });
const make = (query) => run('--auto-create-missing-references', query);

// One case per entry: a name and its steps.
const groups = {
  'command line': [
    ['help', [run('--help')]],
    ['short help', [run('-h')]],
    ['an unknown option', [run('--bogus')]],
    ['an extra positional argument', [run('x', 'y')]],
    ['an option without its value', [run('--db')]],
    ['two trigger commands', [run('--always', '--once', '() ((1 1))')]],
    ['--serve with --connect', [run('--serve', '127.0.0.1:0', '--connect', '127.0.0.1:1')]],
    ['--indexed-lookups with --connect', [run('--connect', '127.0.0.1:1', '--indexed-lookups')]],
    ['an invalid --commit-mode', [run('--commit-mode', 'later', '() ((1 1))')]],
    ['an invalid --retention', [run('--retention', 'forever', '() ((1 1))')]],
    ['an invalid --protocol', [run('--serve', '127.0.0.1:0', '--protocol', 'json')]],
    ['--serve with a query', [run('--serve', '127.0.0.1:0', '() ((1 1))')]],
    ['--forward without --serve', [run('--forward', '127.0.0.1:1')]],
    [
      '--forward with --graphql',
      [run('--serve', '127.0.0.1:0', '--forward', '127.0.0.1:1', '--graphql')],
    ],
    ['--grant-token without --tokens', [run('--grant-token', 'secret')]],
    ['an invalid --arity', [run('--connect', '127.0.0.1:1', '--arity', 'many')]],
    ['--option=value and aliases', [run('--data=d.links', '--do=() ((1 1))', '--links')]],
    ['an empty database', [run('--after')]],
    ['no arguments at all', [run()]],
  ],
  'query language': [
    ['create a point', [make('() ((1 1))'), run('--after')]],
    [
      'create, update and delete doublets',
      [
        run('() ((1 1))', '--changes', '--after'),
        run('() ((2 2))', '--changes', '--after'),
        run('((1: 1 1)) ((1: 1 2))', '--changes', '--after'),
        run('((1 2) (2 2)) ()', '--changes', '--after'),
      ],
    ],
    [
      'several links at once',
      [
        run('() ((1 1) (2 2))', '--changes', '--after'),
        run('((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1))', '--changes', '--after'),
        run('((1: 2 1) (2: 1 2)) ()', '--changes', '--after'),
      ],
    ],
    [
      'variables swap every link',
      [
        run('() ((1 1) (2 2) (1 2))'),
        run('((($index: $source $target)) (($index: $target $source)))', '--changes', '--after'),
        run('((($i: $s $t)) (($i: $s $t)))', '--changes', '--after'),
        run('((($i:)) (($i:)))', '--changes'),
      ],
    ],
    ['delete everything', [run('() ((1 1) (2 2))'), run('((* *)) ()', '--changes', '--after')]],
    [
      'nested links',
      [run('() (((m a) (m a)))', '--after'), run('(((m a) (m a))) (((p a) (p a)))', '--after')],
    ],
    ['a missing reference', [run('() ((child: father mother))')]],
    ['a missing numeric reference', [run('() ((7 8))')]],
    ['a parse error', [run('(( 1 1')]],
    [
      '--before, --changes and --after together',
      [make('() ((1 1))'), run('--before', '--changes', '--after', '() ((2 2))')],
    ],
    [
      '--structure',
      [make('() ((child: father mother))'), run('--structure', '3'), run('--structure', '99')],
    ],
    ['--trace', [run('--trace', '() ((1 1))', '--changes')]],
  ],
  names: [
    [
      'named links',
      [
        make('() ((child: father mother))'),
        run('--after'),
        run('(((child: father mother)) ((child: father mother)))', '--changes'),
        run('((child: father mother)) ((child: mother father))', '--changes', '--after'),
        run('((child: mother father)) ()', '--changes', '--after'),
      ],
    ],
    [
      'unicode names',
      [make('() ((ребёнок: отец мать) (子: 父 母) (emoji🙂: a b))'), run('--after')],
    ],
    ['quoted names', [make("() (('with space': 'a b' \"c d\"))"), run('--after')]],
    [
      '--indexed-lookups',
      [make('() ((child: father mother))'), run('--indexed-lookups', '--after')],
    ],
  ],
  'LiNo and binary import and export': [
    [
      'export and import LiNo',
      [
        make('() ((child: father mother))'),
        run('--export', 'out.lino'),
        run('--db', 'copy.links', '--import', 'out.lino', '--after'),
      ],
    ],
    [
      'import a hand-written LiNo file',
      [
        {
          args: [
            '--db',
            'data.links',
            '--auto-create-missing-references',
            '--in',
            'pages.lino',
            '--out',
            'copy.lino',
            '--after',
          ],
          files: { 'pages.lino': '(home: page title)\n(about: page title)\n(1: 1 1)\n' },
        },
      ],
    ],
    ['import a missing file', [run('--in', 'missing.lino')]],
    ['import a directory', [{ args: ['--in', 'folder'], files: { 'folder/x': '' } }]],
    [
      'export and import a store archive',
      [
        make('() ((child: father mother))'),
        run('--export-binary', 'family.bin'),
        run('--db', 'copy.links', '--import-binary', 'family.bin', '--after'),
      ],
    ],
    ['import a missing archive', [run('--import-binary', 'missing.bin')]],
  ],
  'persistent transformations': [
    [
      'an always trigger fires on later writes',
      [
        make('() ((1: 1 1))'),
        run('--always', '(((1: 1 1)) ((1: 1 2)))'),
        run('((1: 1 2)) ((1: 1 1))', '--after'),
        run('--never', '(((1: 1 1)) ((1: 1 2)))'),
        run('((1: 1 2)) ((1: 1 1))', '--after'),
      ],
    ],
    [
      'a once trigger deletes itself',
      [
        make('() ((1: 1 1))'),
        run('--once', '(((1: 1 1)) ((1: 1 2)))'),
        run('((1: 1 2)) ((1: 1 1))', '--after'),
        run('((1: 1 2)) ((1: 1 1))', '--after'),
      ],
    ],
    [
      'embedded triggers',
      [
        make('() ((1: 1 1))'),
        run('--embed-triggers', '--always', '(((1: 1 1)) ((1: 1 2)))'),
        run('--embed-triggers', '--after'),
      ],
    ],
    [
      'a triggers file',
      [
        make('() ((1: 1 1))'),
        run('--triggers-file', 't.links', '--always', '(((1: 1 1)) ((1: 1 2)))'),
        run('--triggers-file', 't.links', '((1: 1 2)) ((1: 1 1))', '--after'),
      ],
    ],
  ],
  permissions: [
    [
      'a subject sees and writes what its rules allow',
      [
        make(
          '() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) (AllowWrite: AllowWrite AllowWrite) (AllowReference: AllowReference AllowReference) (Anything: Anything Anything) (User: Type User) (Admin: User Admin) (Doc: Type Doc) (Public: Type Public) (Note: Doc Note) (Secret: Doc Secret) (alice: User alice) (boss: Admin boss) (note: Note note) (secret: Secret secret) (news: Public news) (AllowWrite (Admin Anything)) (AllowWrite (User Note)) (AllowRead (User Public)))',
        ),
        run('--permissions', 'self', '--subject', 'alice', '--after'),
        run(
          '--permissions',
          'self',
          '--subject',
          'alice',
          '--changes',
          '((note: Note note)) ((note: Public note))',
        ),
        run('--permissions', 'self', '--subject', 'alice', '--changes', '((secret: * *)) ()'),
        run('--permissions', 'self', '--subject', 'boss', '--after'),
        run('--permissions', 'self', '--subject', 'nobody', '--after'),
        run('--permissions', 'self', '--always', '() ((1 1))'),
        run('--tokens', 'tokens.links', '--subject', 'alice', '--grant-token', 'alice-secret'),
        run('--tokens', 'tokens.links', '--revoke-token', 'alice-secret'),
        run('--tokens', 'self', '--subject', 'boss', '--grant-token', 'boss-secret'),
        run('--tokens', 'self', '--grant-token', 'guest-secret'),
      ],
    ],
  ],
};

for (const [group, cases] of Object.entries(groups)) {
  describe(`differential: ${group}`, () => {
    for (const [name, steps] of cases) {
      it(name, { skip }, () => differential(steps));
    }
  });
}

describe('differential: transactions and version control', () => {
  const options = { mask: maskClock, opaqueFiles: CLOCK_FILES };
  it('transactions and the log', { skip }, () =>
    differential(
      [
        run('--transactions', '--auto-create-missing-references', '() ((1 1))', '--changes'),
        run('--transactions', '() ((2 2))'),
        run('--transactions-file', 'log.links', '() ((3 3))'),
        run('--log'),
        run('--commit-mode', 'async', '((1: 1 1)) ()', '--after'),
        run('--retention', 'sized:2', '() ((4 4))'),
      ],
      options,
    ),
  );
  it('branches, tags and checkout', { skip }, () =>
    differential(
      [
        run('--vc', '--auto-create-missing-references', '() ((1 1))'),
        run('--vc', '() ((2 2))'),
        run('--vc', '--tag', 'v1'),
        run('--vc', '--tag', 'first=1'),
        run('--vc', '--list-tags'),
        run('--vc', '--list-branches'),
        run('--vc', '--checkout', '1', '--after'),
        run('--vc', '--checkout', 'v1', '--after'),
        run('--vc', '--checkout', 'nowhere'),
        run('--branch', 'feature', '--branch-from', '1'),
        run('--branch', 'feature', '() ((3 3))', '--after'),
        run('--vc', '--list-branches'),
        run('--branch', 'main', '--after'),
      ],
      options,
    ),
  );
});
