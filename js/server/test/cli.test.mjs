// The clink command line: every flag, alias, inline form, default and error
// text of rust/src/cli.rs. The error texts and the help text are what the Rust
// clink prints (test/fixtures/clink-help.txt is its `--help` output).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, it } from 'node:test';
import { ArityRange } from 'links-notation';
import { Cli } from '../src/cli.js';

const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));

const run = (...args) => {
  const parsed = Cli.parse(args);
  assert.equal(parsed.kind, 'run');
  return parsed.cli;
};

const RUST_ERRORS = [
  [['--bogus'], "unknown option '--bogus'"],
  [['-x'], "unknown option '-x'"],
  [['-'], "unknown option '-'"],
  [['--graphql=1'], "unknown option '--graphql=1'"],
  [['--help=1'], "unknown option '--help=1'"],
  [['-d=x'], "unknown option '-d=x'"],
  [['--db'], "missing value for option '--db'"],
  [['-d'], "missing value for option '-d'"],
  [['--data'], "missing value for option '--data'"],
  [['--query'], "missing value for option '--query'"],
  [['-q'], "missing value for option '-q'"],
  [['--structure'], "missing value for option '--structure'"],
  [['-s', 'abc'], "invalid link id 'abc' for -s"],
  [['--structure', '-1'], "invalid link id '-1' for --structure"],
  [['-s', '4294967296'], "invalid link id '4294967296' for -s"],
  [['--structure=x'], "invalid link id 'x' for --structure"],
  [['--structure=+5', '--bogus'], "unknown option '--bogus'"],
  [['--trace=maybe'], "invalid boolean value 'maybe' for --trace"],
  [['--trace='], "invalid boolean value '' for --trace"],
  [['--links=nah'], "invalid boolean value 'nah' for --after"],
  [['--after=nah'], "invalid boolean value 'nah' for --after"],
  [['--indexed-lookups=2'], "invalid boolean value '2' for --indexed-lookups"],
  [['--branch-from', 'x'], "invalid sequence value 'x' for --branch-from"],
  [
    ['--branch-from=9223372036854775808'],
    "invalid sequence value '9223372036854775808' for --branch-from",
  ],
  [
    ['--branch-from=-9223372036854775809'],
    "invalid sequence value '-9223372036854775809' for --branch-from",
  ],
  [['--branch-from', '1.5'], "invalid sequence value '1.5' for --branch-from"],
  [['--branch-from'], "missing value for option '--branch-from'"],
  [['--arity'], "missing value for option '--arity'"],
  [['--arity', '0'], "invalid value for '--arity': arity must be at least 1"],
  [
    ['--arity', 'x'],
    "invalid value for '--arity': invalid arity 'x': expected n, min..max or min..",
  ],
  [['--arity', '3..2'], "invalid value for '--arity': arity range 3..2 is empty"],
  [['--arity', '0..'], "invalid value for '--arity': arity must be at least 1"],
  [
    ['--arity=1152921504606846976'],
    "invalid value for '--arity': arity 1152921504606846976 is too large",
  ],
  [
    ['--arity', '1152921504606846976..'],
    "invalid value for '--arity': arity 1152921504606846976 is too large",
  ],
  [
    ['--arity', '2..3..4'],
    "invalid value for '--arity': invalid arity '2..3..4': expected n, min..max or min..",
  ],
  [['--arity', ''], "invalid value for '--arity': invalid arity '': expected n, min..max or min.."],
  [
    ['--arity', ' 2..x '],
    "invalid value for '--arity': invalid arity '2..x': expected n, min..max or min..",
  ],
  [
    ['--arity', '18446744073709551616'],
    "invalid value for '--arity': invalid arity '18446744073709551616': expected n, min..max or min..",
  ],
  [
    ['--arity', '-1'],
    "invalid value for '--arity': invalid arity '-1': expected n, min..max or min..",
  ],
  [
    ['--arity', '..3'],
    "invalid value for '--arity': invalid arity '..3': expected n, min..max or min..",
  ],
  [['a', 'b'], "unexpected extra positional argument 'b'"],
  [['--', 'a', 'b'], "unexpected extra positional argument 'b'"],
  [['a', '--', 'b'], "unexpected extra positional argument 'b'"],
  [['--tokens'], "missing value for option '--tokens'"],
  [['--token'], "missing value for option '--token'"],
  [['--serve'], "missing value for option '--serve'"],
  [['--deep-package'], "missing value for option '--deep-package'"],
  [['--triggers-file'], "missing value for option '--triggers-file'"],
  [['--commit-mode'], "missing value for option '--commit-mode'"],
  [['--vc', '--bogus'], "unknown option '--bogus'"],
  [['-t', 'TRUE', '--bogus'], "unknown option '--bogus'"],
  [['--grant-token'], "missing value for option '--grant-token'"],
  [['--revoke-token'], "missing value for option '--revoke-token'"],
  [['--subject'], "missing value for option '--subject'"],
  [['--permissions'], "missing value for option '--permissions'"],
  [['--protocol'], "missing value for option '--protocol'"],
  [['--connect'], "missing value for option '--connect'"],
  [['--forward'], "missing value for option '--forward'"],
  [['--jwt-secret'], "missing value for option '--jwt-secret'"],
  [['--graphql-admin-secret'], "missing value for option '--graphql-admin-secret'"],
  [['--retention'], "missing value for option '--retention'"],
  [['--vc-file'], "missing value for option '--vc-file'"],
  [['--branch'], "missing value for option '--branch'"],
  [['--checkout'], "missing value for option '--checkout'"],
  [['--tag'], "missing value for option '--tag'"],
  [['--transactions-file'], "missing value for option '--transactions-file'"],
  [['--in'], "missing value for option '--in'"],
  [['--out'], "missing value for option '--out'"],
  [['--export-binary'], "missing value for option '--export-binary'"],
  [['--import-binary'], "missing value for option '--import-binary'"],
  [['--binary-in'], "missing value for option '--binary-in'"],
  [['--export'], "missing value for option '--export'"],
  [['--triplets=x'], "invalid boolean value 'x' for --triplets"],
  [['--packed-widths=x'], "invalid boolean value 'x' for --packed-widths"],
  [['--external-references=x'], "invalid boolean value 'x' for --external-references"],
  [['--log=x'], "invalid boolean value 'x' for --log"],
  [['--list-tags=x'], "invalid boolean value 'x' for --list-tags"],
  [['--list-branches=x'], "invalid boolean value 'x' for --list-branches"],
  [['--vc=x'], "invalid boolean value 'x' for --vc"],
  [['--transactions=x'], "invalid boolean value 'x' for --transactions"],
  [['--embed-triggers=x'], "invalid boolean value 'x' for --embed-triggers"],
  [['--triggers=x'], "invalid boolean value 'x' for --triggers"],
  [['--never=x'], "invalid boolean value 'x' for --never"],
  [['--once=x'], "invalid boolean value 'x' for --once"],
  [['--always=x'], "invalid boolean value 'x' for --always"],
  [['--changes=x'], "invalid boolean value 'x' for --changes"],
  [['--before=x'], "invalid boolean value 'x' for --before"],
  [
    ['--auto-create-missing-references=x'],
    "invalid boolean value 'x' for --auto-create-missing-references",
  ],
  [['--bogus', '--help'], "unknown option '--bogus'"],
  [['--=x'], "unknown option '--=x'"],
];

describe('Cli.parse', () => {
  it('keeps the Rust defaults', () => {
    const cli = run();
    assert.equal(cli.db, 'db.links');
    for (const field of ['query', 'queryArg', 'structure', 'linoInput', 'linoOutput']) {
      assert.equal(cli[field], null, field);
    }
    for (const field of ['binaryInput', 'binaryOutput', 'triggersFile', 'transactionsFile']) {
      assert.equal(cli[field], null, field);
    }
    for (const field of ['commitMode', 'retention', 'vcFile', 'branch', 'branchFrom', 'checkout']) {
      assert.equal(cli[field], null, field);
    }
    for (const field of ['tag', 'forward', 'jwtSecret', 'graphqlAdminSecret', 'connect']) {
      assert.equal(cli[field], null, field);
    }
    for (const field of ['protocol', 'permissions', 'subject', 'tokens', 'grantToken']) {
      assert.equal(cli[field], null, field);
    }
    assert.equal(cli.revokeToken, null);
    assert.equal(cli.token, null);
    assert.equal(cli.lock, null);
    assert.equal(cli.lockTimeout, null);
    for (const field of [
      'trace',
      'indexedLookups',
      'autoCreateMissingReferences',
      'before',
      'changes',
      'after',
      'always',
      'once',
      'never',
      'triggers',
      'embedTriggers',
      'transactions',
      'vc',
      'listBranches',
      'listTags',
      'showLog',
      'graphql',
      'externalReferences',
      'packedWidths',
      'triplets',
      'lockStatus',
    ]) {
      assert.equal(cli[field], false, field);
    }
    assert.deepEqual(cli.serve, []);
    assert.deepEqual(cli.deepPackages, []);
    assert.deepEqual(cli.arity, new ArityRange(2n, 2n));
    assert.equal(cli.triggerCommandCount(), 0);
    assert.equal(cli.vcRequested(), false);
    assert.equal(cli.transactionsRequested(), false);
    assert.equal(cli.persistentTransformationsRequested(), false);
    assert.deepEqual(cli.serveEndpoints(), []);
    assert.deepEqual(cli.serveAlso, []);
  });

  it('takes every string option under each of its spellings', () => {
    const spellings = {
      db: ['-d', '--db', '--data-source', '--data'],
      query: ['-q', '--query', '--apply', '--do'],
      linoOutput: ['--out', '--lino-output', '--export'],
      linoInput: ['--in', '--lino-input', '--import'],
      binaryOutput: ['--export-binary', '--binary-output', '--binary-out'],
      binaryInput: ['--import-binary', '--binary-input', '--binary-in'],
      triggersFile: ['--triggers-file'],
      transactionsFile: ['--transactions-file'],
      commitMode: ['--commit-mode'],
      retention: ['--retention'],
      vcFile: ['--vc-file'],
      branch: ['--branch'],
      checkout: ['--checkout'],
      tag: ['--tag'],
      jwtSecret: ['--jwt-secret'],
      graphqlAdminSecret: ['--graphql-admin-secret'],
      forward: ['--forward'],
      connect: ['--connect'],
      protocol: ['--protocol'],
      permissions: ['--permissions'],
      subject: ['--subject'],
      tokens: ['--tokens'],
      grantToken: ['--grant-token'],
      revokeToken: ['--revoke-token'],
      token: ['--token'],
      lock: ['--lock'],
      lockTimeout: ['--lock-timeout'],
    };
    for (const [field, names] of Object.entries(spellings)) {
      for (const option of names) {
        assert.equal(run(option, '-v=x')[field], '-v=x', option);
        if (option.startsWith('--')) {
          assert.equal(run(`${option}=a=b`)[field], 'a=b', `${option}=`);
          assert.equal(run(`${option}=`)[field], '', `${option}= empty`);
        }
      }
    }
    assert.equal(run('--db', 'a', '-d', 'b').db, 'b', 'the last value wins');
  });

  it('accepts a bounded integer as the LiNo import depth', () => {
    assert.equal(run().linoInputMaxDepth, 64);
    assert.equal(run('--in-max-depth=128').linoInputMaxDepth, 128);
    assert.equal(run('--in-max-depth', '1').linoInputMaxDepth, 1);
    for (const invalid of ['0', '129', '-1', '1.5', 'abc', '']) {
      assert.throws(() => run('--in-max-depth', invalid), {
        message: "invalid value for '--in-max-depth': expected an integer from 1 to 128",
      });
    }
  });

  it('takes --no-lock as --lock none, the last of them winning (#194)', () => {
    assert.equal(run('--no-lock').lock, 'none');
    assert.equal(run('--no-lock', '--lock', 'pid').lock, 'pid');
    assert.equal(run('--lock', 'pid', '--no-lock').lock, 'none');
    // A word after it is the query, not a value.
    assert.equal(run('--no-lock', 'true').queryArg, 'true');
  });

  it('takes every boolean flag, with an optional literal after it or inline', () => {
    const spellings = {
      indexedLookups: ['--indexed-lookups'],
      trace: ['-t', '--trace'],
      autoCreateMissingReferences: ['--auto-create-missing-references'],
      before: ['-b', '--before'],
      changes: ['-c', '--changes'],
      after: ['-a', '--after', '--links'],
      always: ['--always'],
      once: ['--once'],
      never: ['--never'],
      triggers: ['--triggers'],
      embedTriggers: ['--embed-triggers'],
      transactions: ['--transactions'],
      vc: ['--vc'],
      listBranches: ['--list-branches'],
      listTags: ['--list-tags'],
      showLog: ['--log'],
      graphql: ['--graphql'],
      externalReferences: ['--external-references'],
      packedWidths: ['--packed-widths'],
      triplets: ['--triplets'],
      lockStatus: ['--lock-status'],
    };
    for (const [field, names] of Object.entries(spellings)) {
      for (const option of names) {
        assert.equal(run(option)[field], true, option);
        assert.equal(run(option, 'query')[field], true, `${option} query`);
        assert.equal(run(option, 'query').queryArg, 'query', 'a non-literal is the query');
        for (const literal of ['true', '1', 'yes', 'on', 'TRUE', 'Yes', 'oN']) {
          const cli = run(option, literal);
          assert.equal(cli[field], true, `${option} ${literal}`);
          assert.equal(cli.queryArg, null);
        }
        for (const literal of ['false', '0', 'no', 'off', 'FALSE', 'No', 'OFF']) {
          assert.equal(run(option, literal)[field], false, `${option} ${literal}`);
        }
        if (option.startsWith('--') && option !== '--graphql') {
          assert.equal(run(`${option}=yes`)[field], true, `${option}=yes`);
          assert.equal(run(`${option}=Off`)[field], false, `${option}=Off`);
        }
      }
    }
    // Only ASCII letters fold: the Kelvin sign is not a `k`, so this is a query.
    assert.equal(run('-t', 'oK').queryArg, 'oK');
  });

  it('parses --structure as a u32 link id', () => {
    assert.equal(run('-s', '5').structure, 5);
    assert.equal(run('--structure', '4294967295').structure, 4294967295);
    assert.equal(run('--structure=+5').structure, 5);
  });

  it('parses --branch-from as an i64 sequence point', () => {
    assert.equal(run('--branch-from', '-3').branchFrom, -3n);
    assert.equal(run('--branch-from=+7').branchFrom, 7n);
    assert.equal(run('--branch-from=9223372036854775807').branchFrom, 9223372036854775807n);
    assert.equal(run('--branch-from=-9223372036854775808').branchFrom, -9223372036854775808n);
  });

  it('parses --arity as a trimmed arity range', () => {
    assert.deepEqual(run('--arity', '3').arity, new ArityRange(3n, 3n));
    assert.deepEqual(run('--arity', ' 1..4\n').arity, new ArityRange(1n, 4n));
    assert.deepEqual(run('--arity=2..').arity, new ArityRange(2n, null));
    assert.deepEqual(run('--arity', '+2..+2').arity, new ArityRange(2n, 2n));
    assert.deepEqual(
      run('--arity', '1152921504606846975..18446744073709551615').arity,
      new ArityRange(1152921504606846975n, 18446744073709551615n),
    );
  });

  it('collects every --serve endpoint and --deep-package', () => {
    const cli = run('--serve', 'a', '--serve=b', '--serve', 'c', '--deep-package', 'p');
    assert.deepEqual(cli.serve, ['a', 'b', 'c']);
    assert.deepEqual(cli.serveEndpoints(), ['a', 'b', 'c']);
    assert.deepEqual(cli.serveAlso, ['b', 'c']);
    cli.serveEndpoints().push('d');
    assert.deepEqual(cli.serve, ['a', 'b', 'c'], 'the endpoints are a copy');
    assert.deepEqual(run('--deep-package=x', '--deep-package', 'y').deepPackages, ['x', 'y']);
  });

  it('takes one positional query, everything after -- included', () => {
    assert.equal(run('() ((1 1))').queryArg, '() ((1 1))');
    assert.equal(run('--', '--help').queryArg, '--help');
    assert.equal(run('-t', '--', '-x').queryArg, '-x');
    assert.equal(run('a', '--').queryArg, 'a');
    assert.equal(run('--', 'a').queryArg, 'a');
  });

  it('stops at help and version', () => {
    assert.deepEqual(Cli.parse(['-h']), { kind: 'help' });
    assert.deepEqual(Cli.parse(['--help']), { kind: 'help' });
    assert.deepEqual(Cli.parse(['-V']), { kind: 'version' });
    assert.deepEqual(Cli.parse(['--version']), { kind: 'version' });
    assert.deepEqual(Cli.parse(['--help', '--bogus']), { kind: 'help' });
    assert.deepEqual(Cli.parse(['--version', '--help']), { kind: 'version' });
  });

  it('reports a bad command line as Rust does', () => {
    for (const [args, message] of RUST_ERRORS) {
      assert.throws(() => Cli.parse(args), { message }, args.join(' '));
    }
  });

  it('parseFrom skips the program name', () => {
    assert.equal(Cli.parseFrom(['clink', '-t', 'q']).cli.queryArg, 'q');
    assert.deepEqual(Cli.parseFrom(['clink', '--help']), { kind: 'help' });
    assert.equal(Cli.parseFrom([]).cli.db, 'db.links');
  });
});

describe('Cli texts', () => {
  it('prints the Rust help text', () => {
    const expected = readFileSync(new URL('./fixtures/clink-help.txt', import.meta.url), 'utf8');
    assert.equal(Cli.helpText(), expected);
  });

  it('prints the package version', () => {
    assert.equal(Cli.versionText(), `clink ${pkg.version}`);
  });
});

describe('Cli predicates', () => {
  it('counts the trigger commands', () => {
    assert.equal(run('--always').triggerCommandCount(), 1);
    assert.equal(run('--always', '--once', '--never').triggerCommandCount(), 3);
    assert.equal(run('--once', 'false').triggerCommandCount(), 0);
  });

  it('sees the version-control family', () => {
    for (const args of [
      ['--vc'],
      ['--vc-file', 'f'],
      ['--branch', 'b'],
      ['--branch-from', '0'],
      ['--checkout', 'c'],
      ['--tag', 't'],
      ['--list-branches'],
      ['--list-tags'],
    ]) {
      const cli = run(...args);
      assert.equal(cli.vcRequested(), true, args.join(' '));
      assert.equal(cli.transactionsRequested(), true, args.join(' '));
    }
    assert.equal(run('--vc', 'off').vcRequested(), false);
  });

  it('sees the transactions family', () => {
    for (const args of [
      ['--transactions'],
      ['--transactions-file', 'f'],
      ['--commit-mode', 'm'],
      ['--retention', 'r'],
      ['--log'],
    ]) {
      const cli = run(...args);
      assert.equal(cli.transactionsRequested(), true, args.join(' '));
      assert.equal(cli.vcRequested(), false, args.join(' '));
    }
  });

  it('sees the persistent transformation family', () => {
    for (const args of [
      ['--always'],
      ['--once'],
      ['--never'],
      ['--triggers'],
      ['--embed-triggers'],
      ['--triggers-file', 'f'],
    ]) {
      assert.equal(run(...args).persistentTransformationsRequested(), true, args.join(' '));
    }
  });
});
