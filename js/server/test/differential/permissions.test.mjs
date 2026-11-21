// The JavaScript clink against the Rust clink on permissions, tokens and
// persistent transformation triggers: the scenarios of
// rust/tests/cli_permissions_tests.rs and rust/tests/cli_triggers_tests.rs,
// compared byte for byte (standard output, standard error, exit status and
// every file). Skipped without CLINK_RUST or without bin/clink.js.
import { existsSync } from 'node:fs';
import { describe, it } from 'node:test';
import { differential, JS, skip as noRust } from './harness.mjs';

const skip = noRust || (existsSync(JS) ? false : `${JS} is missing`);

const run = (...args) => ({ args });
const make = (query) => run('--auto-create-missing-references', query);
const as = (subject, ...args) => run('--permissions', 'self', '--subject', subject, ...args);

const WORLD =
  '() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) ' +
  '(AllowWrite: AllowWrite AllowWrite) (AllowReference: AllowReference AllowReference) ' +
  '(Anything: Anything Anything) (User: Type User) (Admin: User Admin) (Doc: Type Doc) ' +
  '(Public: Type Public) (Note: Doc Note) (Secret: Doc Secret) (alice: User alice) ' +
  '(boss: Admin boss) (note: Note note) (secret: Secret secret) (news: Public news) ' +
  '(AllowWrite (Admin Anything)) (AllowWrite (User Note)) (AllowRead (User Public)))';
const TRIGGER = '(((1: 1 1)) ((1: 1 2)))';
const SEED = run('--query', '() ((1 1) (2 2))');

const groups = {
  permissions: [
    [
      'a subject sees what its rules allow and writes nothing else',
      [
        make(WORLD),
        as('alice', '--after'),
        as('13', '--after'),
        as(' alice ', '--after'),
        as('alice', '--changes', '((note: Note note)) ((note: Public note))'),
        as('alice', '--changes', '((secret: * *)) ()'),
        as('13', '--changes', '((secret: * *)) ()'),
        as('alice', '() ((Note secret))'),
        as('alice', '() ((Note nothing))'),
        as('boss', '--changes', '((note: Note note)) ((note: Secret note))'),
        as('boss', '--after'),
        as('boss', '--changes', '((note: Secret note)) ((note: Note note))'),
        run('--after'),
      ],
    ],
    [
      'rules apart',
      [
        make(WORLD),
        run('--permissions', 'rules.links', '--subject', 'alice', '--after'),
        run('--permissions', 'rules.links', '--subject', 'boss', '() ((1 1))'),
        run('--after'),
      ],
    ],
    [
      'a guest reads nothing and creates nothing',
      [
        make(WORLD),
        run('--permissions', 'self', '--after'),
        run('--permissions', 'self', '() ((1 2))'),
        run('--permissions', 'self', '--auto-create-missing-references', '() ((1 2))'),
        as('nobody', '--after'),
        as('99999999999', '--after'),
        as('4294967295', '--after'),
        as('', '--after'),
        run('--after'),
      ],
    ],
    [
      'permissions with triggers',
      [
        make(WORLD),
        run('--permissions', 'self', '--always', TRIGGER),
        run('--permissions', 'self', '--never', TRIGGER),
      ],
    ],
  ],
  tokens: [
    [
      'token commands need a token table and a subject',
      [
        make(WORLD),
        run('--subject', 'alice', '--grant-token', 'x'),
        run('--revoke-token', 'x'),
        run('--tokens', 'self', '--grant-token', 'x'),
        run('--tokens', 'self', '--revoke-token', 'x'),
        run('--tokens', 'self', '--subject', 'nobody', '--grant-token', 'x'),
      ],
    ],
    [
      'tokens in the data',
      [
        make(WORLD),
        run('--tokens', 'self', '--subject', 'alice', '--grant-token', 'alice-secret'),
        run('--tokens', 'self', '--subject', 'alice', '--grant-token', 'alice-secret'),
        run('--tokens', 'self', '--subject', 'boss', '--grant-token', 'alice-secret'),
        run('--tokens', 'self', '--subject', '13', '--grant-token', 'second'),
        run('--tokens', 'self', '--revoke-token', 'alice-secret'),
        run('--tokens', 'self', '--revoke-token', 'alice-secret'),
        run('--after'),
      ],
    ],
    [
      'tokens apart',
      [
        make(WORLD),
        run('--tokens', 'tokens.links', '--subject', 'alice', '--grant-token', 'alice-secret'),
        run(
          '--tokens',
          'tokens.links',
          '--subject',
          'alice',
          '--grant-token',
          'alice-secret',
          '--revoke-token',
          'alice-secret',
        ),
        run('--tokens', 'tokens.links', '--revoke-token', 'alice-secret'),
        run('--tokens', 'tokens.links', '--subject', 'boss', '--grant-token', 'boss-secret'),
        run('--db', 'tokens.links', '--after'),
        run('--after'),
      ],
    ],
  ],
  triggers: [
    [
      'an always trigger fires on later writes and keeps firing',
      [
        SEED,
        run('--always', TRIGGER),
        run('--always', TRIGGER),
        run('--after'),
        run('--query', '() ((2 1))'),
        run('--after'),
        run('--query', '((1: 1 2)) ((1: 1 1))'),
        run('--query', '() ((2 2))'),
        run('--after'),
      ],
    ],
    [
      'a once trigger fires only once',
      [
        SEED,
        run('--once', TRIGGER),
        run('--query', '() ((2 1))'),
        run('--query', '((1: 1 2)) ((1: 1 1))'),
        run('--query', '() ((2 2))'),
        run('--after'),
      ],
    ],
    [
      'never removes stored triggers',
      [
        SEED,
        run('--never', TRIGGER),
        run('--always', TRIGGER),
        run('--once', TRIGGER),
        run('--never', TRIGGER),
        run('--never', TRIGGER),
        run('--query', '() ((2 1))'),
        run('--after'),
      ],
    ],
    [
      'a triggers file elsewhere',
      [
        SEED,
        run('--triggers-file', 'elsewhere.links', '--once', TRIGGER),
        run('--query', '() ((2 1))'),
        run('--triggers-file', 'elsewhere.links', '--query', '() ((2 3))'),
        run('--after'),
      ],
    ],
    [
      'embedded triggers',
      [
        SEED,
        run('--embed-triggers', '--always', TRIGGER),
        run('--after'),
        run('--embed-triggers', '--query', '() ((2 1))'),
        run('--after'),
      ],
    ],
    [
      'trigger commands without a query, or two of them',
      [
        run('--always'),
        run('--once', '  '),
        run('--never', ''),
        run('--always', '--once', TRIGGER),
        run('--once', '--never', TRIGGER),
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
